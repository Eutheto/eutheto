<!-- SPDX-License-Identifier: Apache-2.0 -->

# Expected benchmark results

This directory will hold reviewed benchmark expectations after real corpora and runners exist. Correctness expectations and performance baselines must remain distinguishable and must identify the corpus/input version plus the scenario, domain, planning, protocol, backend/build, runner, and expected-result versions that affect interpretation.

A performance baseline must also record the toolchain, target and eligible runner class, fixed clock/time zone/locale/seed/thread count/temp policy, warm-up and sampling method, metric, and review-approved threshold. Creating or replacing a baseline requires review of the measured and semantic difference; regenerating or accepting changed output is not sufficient.

Phase 00 intentionally contains no expected answer, timing, threshold, baseline, or claim of a passing benchmark. See the [fixture contract](../../docs/architecture/test-fixtures.md).
