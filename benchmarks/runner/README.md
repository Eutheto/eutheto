<!-- SPDX-License-Identifier: Apache-2.0 -->

# Benchmark runner

`eutheto-phase03-benchmark` runs the checked-in solver-neutral Phase-03 corpus through the production bundled-worker adapter. It requires an absolute verified artifact root, the independently retained manifest SHA-256, the corpus path, and an output path. The canonical `just bench` recipe builds the exact Nix worker and supplies those inputs.

The runner rejects an empty, malformed, incompatible, or unexpectedly solved corpus. Its versioned JSON output records exact artifact, corpus, protocol, backend, adapter, worker, engine, target, fixed-option, model-count, objective/bound, termination, parent timing, worker-native timing, and reproducibility evidence. All candidate and solver claims are labeled raw and unverified; Phase 04 remains the independent acceptance boundary. Phase-03 microbenchmarks define no product latency target or reviewed performance threshold.
