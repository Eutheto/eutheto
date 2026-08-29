<!-- SPDX-License-Identifier: Apache-2.0 -->

# OR-Tools worker tests

This directory is reserved for native worker tests introduced with the real implementation in [Phase 03](../../../docs/roadmap/03-ortools-worker-vertical-slice.md#tests-and-acceptance). Phase 03 owns the worker-level test targets and fixtures placed here. Broader Rust adapter, process-supervisor, packaging, and end-to-end checks remain with the repository surfaces that exercise those contracts.

Tests added here must exercise observable behavior of the actual worker and its approved build, including applicable framing and state-machine failures, bounded input and output, terminal-result behavior, supported model cases, safe diagnostics, cancellation and cleanup, and executable checks against the pinned native dependencies. They must use the authoritative [solver worker protocol](../../../docs/architecture/worker-protocol.md) and the reviewed instance of the [source-contract schema](../source-contract.schema.json), not reproduce either contract independently. The [unapproved example contract](../source-contract.example.json) cannot authorize a build or provide test expectations for unresolved pins, flags, hashes, or linkage.

The [worker overview](../README.md) records the Phase-00 source gate, while the [generated-code and worker-contract discipline](../../../docs/contributors/generated-code-and-contracts.md#worker-protocol-changes) governs protocol fixtures and generated outputs. Tests must report only behavior they actually execute; a directory, fixture, configured target, or successful compile is not a substitute for the Phase-03 worker and packaging acceptance evidence.

Phase 00 intentionally supplies no dummy executable, placeholder native source, synthetic passing test, or test that treats the deliberate configuration stop as worker success. This README is a substantive ownership marker for the roadmap layout, not evidence that any worker behavior or Phase-03 gate passes.

See also the [Phase-00 repository and reproducible-tooling boundary](../../../docs/roadmap/00-repository-and-reproducible-tooling.md#nix-and-native-environment-contract).
