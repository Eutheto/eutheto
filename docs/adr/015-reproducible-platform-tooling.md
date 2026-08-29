<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-015: Reproducible Platform Tooling

- **Status:** Approved
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)
- **Phase contract:** [Phase 00](../roadmap/00-repository-and-reproducible-tooling.md)

## Context

Linux and macOS contributors need pinned, reproducible language and build tooling, while Apple SDK/signing and Windows WebView2/installers/signing depend on native platform facilities. WSL cannot demonstrate native Windows packaging or sidecar behavior.

## Binding decision

> Nix Flakes are canonical for Linux/macOS development; native pinned Windows tooling/CI is authoritative for WebView2, sidecars, installers, and signing.

Linux is the canonical hermetic build environment. On macOS, Nix supplies language/build tools while Xcode SDK and signing remain native. Native Windows CI and documented pinned prerequisites own Windows-specific evidence.

## Consequences

- The flake and lockfile pin supported Nix systems and tools; ambient global tools are not build authority.
- Shell entry is fast and side-effect-free: it does not install, fetch, migrate, generate, or build.
- WSL may run core/CLI checks but cannot satisfy Windows desktop, WebView2, installer, signing, or sidecar gates.
- Native prerequisites and runner gaps remain explicit rather than being hidden by an ambient workstation.
- Release/signing identities and platform artifact choices remain unresolved gates until their owning phase.

## Rejected alternatives

- Treating WSL as authoritative for native Windows artifacts is rejected.
- Performing installs, downloads, generation, migrations, or builds during shell entry is rejected.
- Assuming Nix can replace native Xcode SDK/signing or Windows WebView2/signing evidence is rejected.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
