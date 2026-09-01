<!-- SPDX-License-Identifier: Apache-2.0 -->

# OR-Tools worker boundary

This directory contains the project-owned out-of-process CP-SAT worker defined
by [ADR-004](../../docs/adr/004-ortools-worker.md). The worker is version
`0.1.0`, implements solver-worker protocol 1.1, and targets OR-Tools
`9.15.6755` with a matched protobuf runtime.

## Source contract gate

Production configuration requires an absolute approved contract through
`EUTHETO_ORTOOLS_PHASE3_CONTRACT`. The contract records the exact OR-Tools and
protobuf sources and hashes, protoc/runtime versions, protocol schema digest,
worker identity/version, and reviewed CMake cache entries. The checked-in
`source-contract.example.json` remains deliberately unapproved and cannot
authorize a production build or install.

`EUTHETO_ORTOOLS_DEVELOPMENT_BUILD=ON` is an explicit local-only escape hatch
for compilation and native testing against the pinned dependencies. It
disables all install rules and does not establish source or distribution
approval. `EUTHETO_ORTOOLS_BUILD_TESTS=ON` adds the focused native test target.

The executable communicates only through four-byte big-endian framed generated
protobuf messages on stdin/stdout. Native source consumes the generated policy
projection rather than duplicating protocol limits. The Rust supervisor,
dependency packaging, installed-manifest approval, and domain translation stay
outside this subtree.
