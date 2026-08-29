# SPDX-License-Identifier: Apache-2.0
# Verifies the native Windows development prerequisites. This script never installs them.

[CmdletBinding()]
param()

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$missing = New-Object 'System.Collections.Generic.List[string]'
$notes = New-Object 'System.Collections.Generic.List[string]'

function Add-Missing {
    param([Parameter(Mandatory = $true)][string]$Message)
    [void]$script:missing.Add($Message)
}

function Add-Note {
    param([Parameter(Mandatory = $true)][string]$Message)
    [void]$script:notes.Add($Message)
}

function Find-Application {
    param([Parameter(Mandatory = $true)][string]$Name)
    return Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    try {
        $output = & $Path @Arguments 2>&1
        if ($LASTEXITCODE -ne 0) {
            return $null
        }
        return (($output | ForEach-Object { $_.ToString() }) -join "`n").Trim()
    } catch {
        return $null
    }
}

function Test-VersionedApplication {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$MissingDescription
    )

    $command = Find-Application $Name
    if ($null -eq $command) {
        Add-Missing $MissingDescription
        return $null
    }

    $version = Invoke-Checked $command.Source $Arguments
    if ([string]::IsNullOrWhiteSpace($version)) {
        Add-Missing "$MissingDescription ($Name was found at '$($command.Source)' but did not run successfully)"
        return $null
    }

    Add-Note "$Name: $($version -replace "`r?`n", '; ')"
    return $version
}

Write-Output 'eutheto native Windows prerequisite verifier'
Write-Output 'This command is read-only: it does not install, update, or enable any prerequisite.'

# Read the shared Rust pin from the repository rather than maintaining a second version authority.
$rustToolchainFile = Join-Path $repoRoot 'rust-toolchain.toml'
$rustPin = $null
if (Test-Path -LiteralPath $rustToolchainFile -PathType Leaf) {
    try {
        $rustToolchainText = Get-Content -LiteralPath $rustToolchainFile -Raw
        $rustPinMatch = [regex]::Match($rustToolchainText, '(?m)^\s*channel\s*=\s*"([^"]+)"')
        if ($rustPinMatch.Success) {
            $rustPin = $rustPinMatch.Groups[1].Value
        }
    } catch {
        $rustPin = $null
    }
}
if ([string]::IsNullOrWhiteSpace($rustPin)) {
    Add-Missing "a readable Rust channel in '$rustToolchainFile'"
} else {
    $rustup = Find-Application 'rustup'
    if ($null -ne $rustup) {
        $toolchainList = Invoke-Checked $rustup.Source @('toolchain', 'list')
        $escapedPin = [regex]::Escape($rustPin)
        $installedPin = if ($null -eq $toolchainList) { $null } else { [regex]::Match($toolchainList, "(?m)^($escapedPin(?:-[^\s(]+)?)") }
        if ($null -eq $installedPin -or -not $installedPin.Success) {
            Add-Missing "Rust $rustPin through rustup (rustup is present, but the pinned toolchain is not installed)"
        } else {
            $toolchainName = $installedPin.Groups[1].Value
            $rustVersion = Invoke-Checked $rustup.Source @('run', $toolchainName, 'rustc', '--version')
            $cargoVersion = Invoke-Checked $rustup.Source @('run', $toolchainName, 'cargo', '--version')
            if ($null -eq $rustVersion -or $rustVersion -notmatch "^rustc $escapedPin(?:\s|$)") {
                Add-Missing "a working rustc $rustPin in rustup toolchain '$toolchainName'"
            } else {
                Add-Note "Rust: $rustVersion ($toolchainName)"
            }
            if ([string]::IsNullOrWhiteSpace($cargoVersion)) {
                Add-Missing "Cargo in rustup toolchain '$toolchainName'"
            } else {
                Add-Note "Cargo: $cargoVersion"
            }
        }
    } else {
        $rustcVersion = Test-VersionedApplication 'rustc' @('--version') "Rust $rustPin (rustup or a directly installed rustc)"
        if ($null -ne $rustcVersion -and $rustcVersion -notmatch ('^rustc ' + [regex]::Escape($rustPin) + '(?:\s|$)')) {
            [void]$notes.RemoveAt($notes.Count - 1)
            Add-Missing "Rust $rustPin (found '$rustcVersion')"
        }
        [void](Test-VersionedApplication 'cargo' @('--version') 'Cargo for the pinned Rust toolchain')
    }
}

# Node is an exact native baseline; pnpm is read from packageManager, including its committed integrity.
$requiredNodeVersion = '24.20.0'
$packageFile = Join-Path $repoRoot 'package.json'
$requiredPnpmVersion = $null
if (Test-Path -LiteralPath $packageFile -PathType Leaf) {
    try {
        $packageManifest = Get-Content -LiteralPath $packageFile -Raw | ConvertFrom-Json
        if ($packageManifest.packageManager -match '^pnpm@([^+]+)\+sha512\.') {
            $requiredPnpmVersion = $Matches[1]
        }
    } catch {
        Add-Missing "a parseable '$packageFile'"
    }
}
if ([string]::IsNullOrWhiteSpace($requiredPnpmVersion)) {
    Add-Missing "an integrity-pinned pnpm packageManager entry in '$packageFile'"
}

$nodeVersion = Test-VersionedApplication 'node' @('--version') "Node.js $requiredNodeVersion"
if ($null -ne $nodeVersion -and $nodeVersion.Trim() -ne "v$requiredNodeVersion") {
    [void]$notes.RemoveAt($notes.Count - 1)
    Add-Missing "Node.js $requiredNodeVersion (found '$($nodeVersion.Trim())')"
}

if (-not [string]::IsNullOrWhiteSpace($requiredPnpmVersion)) {
    # Prevent a Corepack shim from downloading pnpm while this verifier probes it.
    $oldCorepackNetwork = [Environment]::GetEnvironmentVariable('COREPACK_ENABLE_NETWORK', 'Process')
    [Environment]::SetEnvironmentVariable('COREPACK_ENABLE_NETWORK', '0', 'Process')
    try {
        $pnpmVersion = Test-VersionedApplication 'pnpm' @('--version') "pnpm $requiredPnpmVersion (already prepared locally; network access is disabled during this check)"
        if ($null -ne $pnpmVersion) {
            $pnpmDetectedVersion = ($pnpmVersion -split "`r?`n" | Select-Object -Last 1).Trim()
            if ($pnpmDetectedVersion -ne $requiredPnpmVersion) {
                [void]$notes.RemoveAt($notes.Count - 1)
                Add-Missing "pnpm $requiredPnpmVersion (found '$pnpmDetectedVersion')"
            }
        }
    } finally {
        [Environment]::SetEnvironmentVariable('COREPACK_ENABLE_NETWORK', $oldCorepackNetwork, 'Process')
    }
}

# Visual Studio Installer includes vswhere. Query the workload component without changing the installation.
$programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
$programFiles = [Environment]::GetEnvironmentVariable('ProgramFiles')
$vswhere = Find-Application 'vswhere'
if ($null -eq $vswhere) {
    $vswhereCandidates = New-Object 'System.Collections.Generic.List[string]'
    if (-not [string]::IsNullOrWhiteSpace($programFilesX86)) {
        [void]$vswhereCandidates.Add((Join-Path $programFilesX86 'Microsoft Visual Studio\Installer\vswhere.exe'))
    }
    if (-not [string]::IsNullOrWhiteSpace($programFiles)) {
        [void]$vswhereCandidates.Add((Join-Path $programFiles 'Microsoft Visual Studio\Installer\vswhere.exe'))
    }
    foreach ($candidate in $vswhereCandidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            $vswhere = Get-Item -LiteralPath $candidate
            break
        }
    }
}

$vsInstallation = $null
if ($null -eq $vswhere) {
    Add-Missing 'Visual Studio Build Tools or IDE installed by Visual Studio Installer, with the Desktop development with C++ workload (vswhere.exe was not found)'
} else {
    $vswherePath = if ($vswhere -is [System.Management.Automation.ApplicationInfo]) { $vswhere.Source } else { $vswhere.FullName }
    $vsInstallationOutput = Invoke-Checked $vswherePath @(
        '-latest',
        '-products', '*',
        '-requires',
        'Microsoft.VisualStudio.Workload.VCTools',
        'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
        '-property', 'installationPath'
    )
    if (-not [string]::IsNullOrWhiteSpace($vsInstallationOutput)) {
        $vsInstallation = ($vsInstallationOutput -split "`r?`n" | Select-Object -First 1).Trim()
    }
    if ([string]::IsNullOrWhiteSpace($vsInstallation)) {
        Add-Missing 'the Visual Studio Desktop development with C++ workload, including MSVC x86/x64 build tools'
    } else {
        $compilerPattern = Join-Path $vsInstallation 'VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe'
        $compiler = Get-ChildItem -Path $compilerPattern -File -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1
        if ($null -eq $compiler) {
            Add-Missing "the MSVC x64 compiler under '$vsInstallation'"
        } else {
            Add-Note "Visual Studio C++: $vsInstallation; compiler: $($compiler.FullName)"
        }
    }
}

# Verify a complete Windows 10/11 SDK by its headers, x64 import library, and resource compiler.
$kitsRoot = $null
$kitsRegistryPaths = @(
    'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows Kits\Installed Roots',
    'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots'
)
foreach ($registryPath in $kitsRegistryPaths) {
    try {
        $candidateRoot = (Get-ItemProperty -LiteralPath $registryPath -Name KitsRoot10 -ErrorAction Stop).KitsRoot10
        if (-not [string]::IsNullOrWhiteSpace($candidateRoot) -and (Test-Path -LiteralPath $candidateRoot -PathType Container)) {
            $kitsRoot = $candidateRoot
            break
        }
    } catch {
        # Continue to the next read-only discovery location.
    }
}
if ([string]::IsNullOrWhiteSpace($kitsRoot) -and -not [string]::IsNullOrWhiteSpace($programFilesX86)) {
    $defaultKitsRoot = Join-Path $programFilesX86 'Windows Kits\10'
    if (Test-Path -LiteralPath $defaultKitsRoot -PathType Container) {
        $kitsRoot = $defaultKitsRoot
    }
}

$sdkMatch = $null
if (-not [string]::IsNullOrWhiteSpace($kitsRoot)) {
    $includeRoot = Join-Path $kitsRoot 'Include'
    if (Test-Path -LiteralPath $includeRoot -PathType Container) {
        foreach ($sdkDirectory in (Get-ChildItem -LiteralPath $includeRoot -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending)) {
            $sdkVersion = $sdkDirectory.Name
            $windowsHeader = Join-Path $sdkDirectory.FullName 'um\Windows.h'
            $sharedHeader = Join-Path $sdkDirectory.FullName 'shared\sdkddkver.h'
            $kernelLibrary = Join-Path $kitsRoot "Lib\$sdkVersion\um\x64\kernel32.lib"
            $resourceCompiler = Join-Path $kitsRoot "bin\$sdkVersion\x64\rc.exe"
            if ((Test-Path -LiteralPath $windowsHeader -PathType Leaf) -and
                (Test-Path -LiteralPath $sharedHeader -PathType Leaf) -and
                (Test-Path -LiteralPath $kernelLibrary -PathType Leaf) -and
                (Test-Path -LiteralPath $resourceCompiler -PathType Leaf)) {
                $sdkMatch = $sdkVersion
                break
            }
        }
    }
}
if ($null -eq $sdkMatch) {
    Add-Missing 'a complete Windows 10 or Windows 11 SDK with headers, x64 libraries, and x64 rc.exe (select it in the Visual Studio C++ workload)'
} else {
    Add-Note "Windows SDK: $sdkMatch ($kitsRoot)"
}

# The Evergreen WebView2 Runtime is a prerequisite, not something this script may bootstrap.
$webView2Version = $null
$webView2RegistryPaths = @(
    'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F1E7E819-0D8D-4B9F-B6F6-8A9B626A5D9D}',
    'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F1E7E819-0D8D-4B9F-B6F6-8A9B626A5D9D}',
    'Registry::HKEY_CURRENT_USER\Software\Microsoft\EdgeUpdate\Clients\{F1E7E819-0D8D-4B9F-B6F6-8A9B626A5D9D}'
)
foreach ($registryPath in $webView2RegistryPaths) {
    try {
        $candidateVersion = (Get-ItemProperty -LiteralPath $registryPath -Name pv -ErrorAction Stop).pv
        if (-not [string]::IsNullOrWhiteSpace($candidateVersion) -and $candidateVersion -ne '0.0.0.0') {
            $webView2Version = $candidateVersion
            break
        }
    } catch {
        # Continue to the file-system check.
    }
}

$webView2Executable = $null
$webView2Roots = New-Object 'System.Collections.Generic.List[string]'
if (-not [string]::IsNullOrWhiteSpace($programFilesX86)) {
    [void]$webView2Roots.Add((Join-Path $programFilesX86 'Microsoft\EdgeWebView\Application'))
}
$localAppData = [Environment]::GetEnvironmentVariable('LOCALAPPDATA')
if (-not [string]::IsNullOrWhiteSpace($localAppData)) {
    [void]$webView2Roots.Add((Join-Path $localAppData 'Microsoft\EdgeWebView\Application'))
}
foreach ($webView2Root in $webView2Roots) {
    if (Test-Path -LiteralPath $webView2Root -PathType Container) {
        $webView2Executable = Get-ChildItem -Path (Join-Path $webView2Root '*\msedgewebview2.exe') -File -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1
        if ($null -ne $webView2Executable) {
            break
        }
    }
}
if ([string]::IsNullOrWhiteSpace($webView2Version) -and $null -eq $webView2Executable) {
    Add-Missing 'the Microsoft Edge WebView2 Evergreen Runtime; Windows 10, Windows Server, and LTSC are not assumed to include it'
} else {
    if ([string]::IsNullOrWhiteSpace($webView2Version)) {
        Add-Note "WebView2 Evergreen Runtime: detected at $($webView2Executable.FullName)"
    } else {
        Add-Note "WebView2 Evergreen Runtime: $webView2Version"
    }
}

[void](Test-VersionedApplication 'cmake' @('--version') 'CMake')
[void](Test-VersionedApplication 'ninja' @('--version') 'Ninja')
$protocVersion = Test-VersionedApplication 'protoc' @('--version') 'protoc (presence only until the Phase-03 OR-Tools/protobuf matched-version gate closes)'
if ($null -ne $protocVersion) {
    Add-Note 'protoc compatibility: detected only; the exact OR-Tools/protobuf set remains a Phase-03 gate'
}
[void](Test-VersionedApplication 'git' @('--version') 'Git')
[void](Test-VersionedApplication 'just' @('--version') 'Just')

Write-Output ''
foreach ($note in $notes) {
    Write-Output "[ok] $note"
}

if ($missing.Count -gt 0) {
    Write-Output ''
    Write-Output "Missing prerequisites ($($missing.Count)):"
    foreach ($item in $missing) {
        Write-Output "[missing] $item"
    }
    Write-Output ''
    Write-Output 'Nothing was installed. Install or repair only the listed prerequisites, then run this verifier again.'
    exit 1
}

Write-Output ''
Write-Output 'All currently verifiable native Windows prerequisites are present.'
Write-Output 'Nothing was installed. WebView2 clean-machine packaging remains a later release gate.'
exit 0
