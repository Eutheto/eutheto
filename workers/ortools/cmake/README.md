<!-- SPDX-License-Identifier: Apache-2.0 -->

# OR-Tools worker CMake modules

This directory is reserved for project-owned CMake modules used by the real native OR-Tools worker in [Phase 03](../../../docs/roadmap/03-ortools-worker-vertical-slice.md#native-build-and-packaging). Phase 03 owns every module added here and must add only modules consumed by the reviewed worker build.

The Phase-03 source gate is recorded by the generated approved [`source-contract.json`](../source-contract.json). The generator-owned [`dependency-sources.json`](../dependency-sources.json) binds the exact transitive archives and upstream patch mapping to that OR-Tools source. Modules must preserve the build boundary described by the [worker overview](../README.md), require an explicit absolute contract path, and authenticate its complete bytes against that generated record before build or install. The [unapproved example contract](../source-contract.example.json) is documentation, not permission to select a pin, flag, hash, target, or linkage mode.

[`native_windows_build.cmake`](native_windows_build.cmake) is the production Windows x86_64 build implementation launched by `cargo xtask solver build-native`. It admits only MSVC/Ninja, downloads the generated fixed-hash source set, performs the same fully disconnected dependency-source overrides and patch order as the Nix derivation, verifies effective CMake state, runs native tests, recursively inspects and installs the x64 project DLL closure, and rejects unclassified imports before installed EOF startup. The exact MSVC 14.0 release-runtime imports measured by the approved candidate are classified as external build dependencies; clean-machine runtime delivery remains a later packaging gate. The output remains a build artifact—not an installed manifest, license/SBOM payload, sidecar, backend registration, or release artifact.

The candidate probe remains explicitly non-distributable historical pre-pin evidence and is not the WORKER-002 native production build gate. Target-specific compiler, generator, Eigen definition spelling, verified fetched-source override, install, and package-location values remain measured build facts for the future installed manifest rather than portable source-contract entries.

Related contracts:

- [Phase-00 repository and reproducible-tooling boundary](../../../docs/roadmap/00-repository-and-reproducible-tooling.md#nix-and-native-environment-contract)
- [solver worker protocol](../../../docs/architecture/worker-protocol.md)
- [generated-code and worker-contract discipline](../../../docs/contributors/generated-code-and-contracts.md#worker-protocol-changes)
