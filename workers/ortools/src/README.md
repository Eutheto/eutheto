<!-- SPDX-License-Identifier: Apache-2.0 -->

# OR-Tools worker native source

This directory is reserved for the real project-owned C++ worker implemented by [Phase 03](../../../docs/roadmap/03-ortools-worker-vertical-slice.md#worker-protocol-and-process-supervisor). Phase 03 owns the native entry point and supporting source placed here; Phase 00 does not provide a compilable source file or worker executable.

The source must implement the bounded, out-of-process worker described by the [worker overview](../README.md) and the authoritative [solver worker protocol](../../../docs/architecture/worker-protocol.md). It remains domain-neutral: it validates native model input, applies only reviewed parameters, communicates only through the versioned worker boundary, emits bounded safe diagnostics, and returns candidates rather than authoritative domain decisions.

Native source may be added only with an approved instance of the [source-contract schema](../source-contract.schema.json), after the Phase-03 source, matched protobuf, callback, assumption-core, license, target, linkage, packaging, and benchmark gates close. The [unapproved example contract](../source-contract.example.json) is not build input and supplies no pin, flag, hash, or implementation choice. Generated protocol sources, when introduced through the repository generation contract, remain generated artifacts rather than hand-authored source; see the [generated-code and worker-contract discipline](../../../docs/contributors/generated-code-and-contracts.md#worker-protocol-changes).

Phase 00 intentionally provides no stub `main`, mock protocol peer, successful no-op, or other dummy native implementation. This README retains the roadmap directory as an ownership boundary only. Neither the directory nor this document demonstrates that a worker compiles, launches, handshakes, solves, packages, or satisfies a Phase-03 exit gate.

See also the [Phase-00 repository and reproducible-tooling boundary](../../../docs/roadmap/00-repository-and-reproducible-tooling.md#nix-and-native-environment-contract).
