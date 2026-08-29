<!-- SPDX-License-Identifier: Apache-2.0 -->

# macOS Development

Nix is the language/build-tool authority on macOS. Apple's SDK, linker integration, WebKit system framework, code signing, and notarization remain native Xcode responsibilities; they are not copied into the Nix store.

Follow [`development.md`](development.md) to enter the default flake shell with or without `direnv`.

## Xcode and SDK prerequisite

Install the full Xcode release approved for the runner/workstation through Apple's or your organization's normal management path. Select it and complete any license or first-launch actions outside repository automation. The project does not use a shell hook or bootstrap recipe to install Xcode, accept its license, change the active developer directory, or create signing identities.

From a normal terminal, collect the native state:

```console
xcode-select --print-path
xcodebuild -version
xcrun --sdk macosx --show-sdk-path
xcrun --sdk macosx --show-sdk-version
xcrun --sdk macosx clang --version
uname -m
```

All five Xcode/SDK queries must succeed for desktop work. An active path that contains only Command Line Tools can be sufficient for limited core compilation, but it is not evidence for the Tauri desktop, packaging, signing, or notarization path. If the SDK query fails, repair/select Xcode through the workstation's approved process and rerun it; do not add a privileged fix to `.envrc`, the Nix shell, or `just bootstrap`.

After confirming Xcode, enter the Nix environment and use canonical commands:

```console
nix develop
just bootstrap
just check
just desktop-dev
```

A development launch is not a signed or notarized artifact. Bundle identifiers, signing custody, entitlements, hardened-runtime choices, notarization, updater artifacts, and clean-machine publication remain release gates.

## Runner architectures

The flake exposes both macOS systems:

| Native runner output | Expected `uname -m` |
|---|---|
| `aarch64-darwin` | `arm64` |
| `x86_64-darwin` | `x86_64` |

The flake pins `nixos-25.11` because it is the newest nixpkgs line verified to evaluate all four Phase-00 systems; the 26.11 `nixos-unstable` snapshot dropped `x86_64-darwin`. This is a four-system compatibility exception, not a claim that 25.11 is newest generally. Re-evaluate the pin when a newer supported line restores `x86_64-darwin` or the project support policy closes that target.

Run each CI lane on a native runner of the matching architecture. On Apple Silicon, an x86_64 process under Rosetta is not a substitute for the native `aarch64-darwin` lane, and it does not prove a separately provisioned native Intel runner. Do not mix Nix store paths, Cargo target output, or pnpm native artifacts between architecture lanes.

The Phase-00 acceptance matrix calls for native arm64 and x86_64 evidence, but a flake output evaluating successfully is not proof that a runner entered the shell, compiled the native target, or launched the desktop. Keep unavailable runner lanes explicitly reported; do not label an unexecuted architecture as supported.

Record `uname -m`, Xcode version, SDK version, selected developer path, Nix system, and the exact `just` recipe with any architecture-specific failure. Do not attach signing identities, keychain contents, provisioning material, or broad environment dumps.

## Core-only work

For Rust/core or the internal CLI without a desktop launch, use the [core-only route](development.md#core-only-route). It still uses the pinned Nix Rust/Node/pnpm tools. Core-only success does not validate AppKit/WebKit integration, app bundles, signing, notarization, universal binaries, or updater behavior.