<!-- SPDX-License-Identifier: Apache-2.0 -->

# Native Windows Development

Native Windows—not WSL—is authoritative for the MSVC desktop build, WebView2 behavior, native sidecars, installers, and signing. WSL may run the [core-only route](development.md#core-only-route), but a successful WSL build is not Windows desktop evidence.

Phase 00 verifies the x64 MSVC development path. It does not claim an installer, signing identity, Windows-on-Arm release, or clean-machine publication support.

## Prerequisites

Prepare these through your organization's normal software-management process:

- Visual Studio or Visual Studio Build Tools with **Desktop development with C++**;
- the MSVC x86/x64 compiler component and a complete Windows 10 or Windows 11 SDK;
- Microsoft Edge WebView2 Evergreen Runtime;
- Rust **1.97.1** with Cargo, matching [`rust-toolchain.toml`](../../rust-toolchain.toml);
- Node.js **24.20.0** and pnpm **11.24.0**, with pnpm matching the integrity-pinned root `packageManager` entry;
- CMake, Ninja, `protoc`, Git, and Just.

The worker build uses the approved OR-Tools 9.15.6755/protobuf 33.1 source contract and ignores any ambient OR-Tools or protobuf installation. Do not independently select a supposedly “latest compatible” protobuf toolchain.

## Run the verifier

From a native Windows PowerShell prompt at the repository root:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\bootstrap-windows.ps1
```

The script is non-privileged and read-only. It:

- reads the Rust and pnpm pins from committed repository files;
- checks an already-installed rustup toolchain without asking rustup to install it;
- disables Corepack network access while probing pnpm, so a missing pnpm package is reported rather than downloaded;
- uses Visual Studio Installer's `vswhere.exe` to verify the C++ component and locates `cl.exe`;
- verifies Windows SDK headers, x64 import libraries, and `rc.exe`;
- checks the machine/user WebView2 Evergreen registration or installed runtime executable; and
- runs version queries for already-present Node, pnpm, CMake, Ninja, `protoc`, Git, and Just.

It prints every detected prerequisite and every missing or unusable prerequisite. Exit code `0` means all currently verifiable prerequisites were found; exit code `1` means the `[missing]` entries must be resolved. It never invokes an installer, package manager install command, Visual Studio modification, rustup installation, Corepack download, elevation, or registry write.

Install or repair only what the report names, then rerun it. Do not add `winget`, Chocolatey, Visual Studio Installer, rustup, or Corepack installation calls to this script: software approval and elevation are workstation policy, not repository bootstrap behavior.

## WebView2 strategy

Development uses an already-installed **Evergreen WebView2 Runtime**. The verifier does not assume that Windows 10, Windows Server, or LTSC contains it, and the presence of the Edge browser is not used as a substitute for an explicit runtime check.

A successful check on a contributor workstation is not the clean-machine gate. The eventual Windows artifact must be tested in a clean supported image without relying on a developer's existing Edge/WebView2 state. The exact runtime delivery choice, offline behavior, artifact licensing, installer integration, updater behavior, and clean-machine publication evidence remain later release gates (Phase 11 packaging and Phase 12 publication). Until those gates close, do not claim Windows installer or offline support.

## Native command path

After the verifier passes, use the repository command authority. Run the worker build from an **x64 Visual Studio developer PowerShell or command prompt** so `cl.exe` and `dumpbin.exe` are active:

```powershell
just bootstrap
just typecheck
just cli-build
just ui-build
just desktop-dev
just worker-build-native
```

`just bootstrap` performs explicit locked dependency preparation; opening PowerShell or running the verifier does not. The other application recipes exercise the implemented native core, frontend, and development desktop paths without requiring Nix-only formatting tools or the optional full-shell Rust test tools. `just desktop-dev` is development exercise only, not packaged-desktop or installer evidence. Run the aggregate `just check` in the locked Nix environment and in CI; do not report it as a native Windows result unless that complete command toolchain was actually provisioned and exercised.

`just worker-build-native` downloads the generated fixed-hash source set, builds the approved Windows x86_64 worker with MSVC/Ninja and the dynamic `MultiThreadedDLL` runtime required across the shared-Protobuf boundary, runs its native tests, recursively derives and inspects the x64 DLL closure, copies only transitively imported app-local DLLs from the verified OR-Tools stage and active MSVC redistributable, rejects every remaining import outside the exact Windows system set, and verifies EOF startup. Rust then verifies reviewed solver-dependency license-source hashes, installs the exact license and project NOTICE payload, generates and validates the artifact-specific SPDX SBOM and solver manifest, and replaces `.cache/ortools-native/windows-x86_64/current` only after staging validation. A failed pre-publication rerun preserves the last-good result, and an interrupted replacement is recovered on the next locked build. This is build-level artifact evidence only; manifest-validated installation, packaged launch/handshake, backend availability, and release preflight remain unavailable until their later work packages.

For portable core/CLI work without native desktop evidence, follow the [core-only route](development.md#core-only-route). Keep its results labeled as core/CLI results.

## What to record when reporting a failure

Include:

- the verifier's `[ok]` and `[missing]` lines;
- native OS edition/build and whether the shell/process is x64;
- Visual Studio edition and installed MSVC/SDK versions;
- whether WebView2 was machine-wide or per-user;
- the exact `just` recipe that failed; and
- whether the failure occurred in native PowerShell or WSL.

Do not include environment dumps, cache archives, credentials, signing material, or unsanitized application data.