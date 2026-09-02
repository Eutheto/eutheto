<!-- SPDX-License-Identifier: Apache-2.0 -->

# OR-Tools worker CMake modules

This directory is reserved for project-owned CMake modules used by the real native OR-Tools worker in [Phase 03](../../../docs/roadmap/03-ortools-worker-vertical-slice.md#native-build-and-packaging). Phase 03 owns every module added here and must add only modules consumed by the reviewed worker build.

The Phase-03 source gate is recorded by the generated approved [`source-contract.json`](../source-contract.json). Modules must preserve the build boundary described by the [worker overview](../README.md), require an explicit absolute contract path, and authenticate its complete bytes against that generated record before build or install. The [unapproved example contract](../source-contract.example.json) is documentation, not permission to select a pin, flag, hash, target, or linkage mode.

Phase 03 must use these modules to build only the required CP-SAT worker components and to support the same project worker source under Nix and the supported native Windows toolchain. Cross-target source choices come from the reviewed source contract. Target-specific compiler, generator, Eigen definition spelling, verified fetched-source override, install, and package-location values remain measured build facts for the installed manifest rather than portable source-contract entries.

The candidate probe remains explicitly non-distributable. Production Nix/native build, installed-manifest, artifact SBOM/license, packaging, and release evidence are still dependency-gated; no module may replace them with a no-op target, dummy executable, or synthetic result.

Related contracts:

- [Phase-00 repository and reproducible-tooling boundary](../../../docs/roadmap/00-repository-and-reproducible-tooling.md#nix-and-native-environment-contract)
- [solver worker protocol](../../../docs/architecture/worker-protocol.md)
- [generated-code and worker-contract discipline](../../../docs/contributors/generated-code-and-contracts.md#worker-protocol-changes)
