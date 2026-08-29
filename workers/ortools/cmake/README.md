<!-- SPDX-License-Identifier: Apache-2.0 -->

# OR-Tools worker CMake modules

This directory is reserved for project-owned CMake modules used by the real native OR-Tools worker in [Phase 03](../../../docs/roadmap/03-ortools-worker-vertical-slice.md#native-build-and-packaging). Phase 03 owns every module added here and must add only modules consumed by the reviewed worker build.

A module belongs here only after the Phase-03 source gate approves the exact OR-Tools and matched protobuf inputs, the supported native toolchains, and the CMake cache entries for that source revision. Modules must preserve the build boundary described by the [worker overview](../README.md) and the approved instance of the [source-contract schema](../source-contract.schema.json). The [unapproved example contract](../source-contract.example.json) is documentation, not permission to select a pin, flag, hash, target, or linkage mode.

Phase 03 must use these modules to build only the required CP-SAT worker components and to support the same project worker source under Nix and the supported native Windows toolchain. Release-specific CMake choices must come from the reviewed source contract; they must not be copied from another OR-Tools release or inferred from this directory.

Phase 00 intentionally provides no helper that creates a no-op target, dummy executable, or synthetic dependency result. This README retains the roadmap directory as an ownership boundary only. Its presence, and the presence of the directory, are not build, packaging, protocol, license, or phase-exit evidence.

Related contracts:

- [Phase-00 repository and reproducible-tooling boundary](../../../docs/roadmap/00-repository-and-reproducible-tooling.md#nix-and-native-environment-contract)
- [solver worker protocol](../../../docs/architecture/worker-protocol.md)
- [generated-code and worker-contract discipline](../../../docs/contributors/generated-code-and-contracts.md#worker-protocol-changes)
