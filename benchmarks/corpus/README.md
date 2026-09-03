<!-- SPDX-License-Identifier: Apache-2.0 -->

# Benchmark corpus

Phase 03 adds the solver-neutral [`solver/phase03-primitives.json`](solver/phase03-primitives.json) corpus. Each case embeds the versioned public `PlanningProblem` representation plus explicit expected raw backend evidence. The cases are synthetic, Apache-2.0 licensed, sorted by stable ID, and contain no backend-native model.

Future domain benchmarks remain grouped under [`workforce/`](workforce/), [`seating/`](seating/), and [`school/`](school/). Every case must use its owning versioned public/domain format, be synthetic or demonstrably sanitized, record its provenance and redistribution license, and avoid backend-native models.
