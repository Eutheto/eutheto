<!-- SPDX-License-Identifier: Apache-2.0 -->

# Benchmarks

This tree reserves the future benchmark contract:

- [`corpus/`](corpus/) owns sanitized, versioned inputs grouped by official domain;
- [`expected/`](expected/) owns reviewed correctness expectations and performance baselines; and
- [`runner/`](runner/) owns benchmark execution and result reporting.

Phase 00 does not provide a solver, benchmark runner, corpus case, expected result, threshold, or performance claim. Directory presence and zero discovered cases are not benchmark evidence. Future additions must follow the [test fixture contracts](../docs/architecture/test-fixtures.md) and the phase that implements the measured behavior.
