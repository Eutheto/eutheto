<!-- SPDX-License-Identifier: Apache-2.0 -->

# Benchmark runner

This directory is reserved for the canonical benchmark runner once the owning implementation phase supplies real scenarios and measurable behavior. The runner must discover an explicit nonzero case set, apply the fixed execution context from the [fixture contract](../../docs/architecture/test-fixtures.md), distinguish correctness failures from performance regressions, and emit versioned results with enough toolchain, target, corpus, protocol, backend/build, and runner identity to reproduce or reject a comparison.

Results from an unapproved or incomparable host must not silently replace reviewed baselines. Phase 00 adds no runner, executable, no-op command, or successful benchmark invocation.
