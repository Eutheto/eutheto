# Cross-Cutting Performance and Solver UX Targets

## Status, source, and authority

This document incorporates **Eutheto Performance and Solver UX Targets**, dated 2026-08-29, from `eutheto-performance-ux-targets.md`, SHA-256 `f5ad3479f76e22a16ca355abd8f3323add9f853003d9590c97d3080c2a3c389a`.

It is the cross-cutting roadmap contract for solve-path latency, solver-budget UX, instrumentation, and representative performance evidence. Phase files remain authoritative for their owned implementation slices; this document prevents those slices from adopting inconsistent timing or status semantics.

All numeric values below are **initial engineering targets**, not universal guarantees or release claims. Phase 12 must replace assumptions with measured, versioned baselines on representative fixtures and a recorded reference machine. Product text must not promise a time that the supported envelope and evidence cannot sustain.

## Product outcome

Eutheto may perform sophisticated combinatorial planning internally, but ordinary use on a contemporary consumer computer should feel fast, bounded, and predictable:

- pressing **Optimize** never blocks the webview or requires an account, network, or AI provider when required normalized data is already local;
- a useful independently verified incumbent is returned promptly instead of being withheld merely to prove global optimality;
- external synchronization and enrichment happen outside the critical path unless the current immutable revision explicitly lacks required data;
- every solve has one end-to-end budget covering preparation, backend execution, fallback, projection, independent verification, and result preparation;
- status and copy distinguish feasible, optimal, proven infeasible, cancelled, and no verified solution within limits;
- pathological search is capped rather than monopolizing the machine indefinitely.

## Reference hardware and measurement boundary

Initial targets assume an ordinary laptop or desktop with approximately 4–8 CPU cores, 16 GiB RAM, no dedicated GPU requirement, the local Tauri application, and local solver execution. The final Phase-12 baseline records the exact CPU, memory, operating system, power mode, toolchain, application build, backend, worker count, and cold/warm cache state.

Measure the user-visible path from accepted **Optimize** action until the verified result is available and the primary result view has rendered. Report separate spans for:

1. immutable-revision capture and validation;
2. domain normalization and candidate generation;
3. local transportation/snapshot lookup and any separately disclosed network enrichment;
4. Planning-IR compilation and validation;
5. backend translation and worker startup;
6. time to first backend incumbent;
7. projection and first independently verified feasible result;
8. continued bounded improvement and final authoritative score;
9. explanation/result post-processing; and
10. primary UI render.

Solver time alone cannot substantiate an end-to-end product claim.

## Initial performance objectives

| Scenario/metric | Initial objective | Interpretation |
|---|---:|---|
| Small/simple warm scenario | **<500 ms end to end** | Few entities, locations, resources, and preferences. |
| Typical warm scenario | **Target <1 s; expected envelope roughly 0.3–1.5 s** | All required provider-derived data is already represented by valid local snapshots/caches. |
| Typical cold scenario | **Usually <3 s** | A bounded amount of genuinely required enrichment may occur; cold/network time is reported separately. |
| 95th percentile normal scenario | **<5 s** | Evaluated over the approved representative normal corpus, not arbitrary files. |
| Moderately complex scenario | **<5 s for the large majority; <10 s expected ceiling** | Responsive progress and cancellation remain mandatory. |
| Large/heavily constrained scenario | **Approximately 4–15 s only in an explicit complex profile** | Not the default interactive experience; evidence and model envelope must justify support. |
| Pathological/extreme input | **Deliberately capped** | Return accurate limit/resource status; never search without a bound. |
| UI responsiveness while solving | **No webview long task or frozen input path** | Rendering, cancellation, navigation, and assistive technology remain usable. |

The source proposal's illustrative CP-SAT range of approximately 0.1–0.8 seconds for a common pack is a hypothesis to test, not a backend SLA. Model construction, worker startup, verification, and rendering remain part of the product budget.

## Budget and incumbent policy

### Interactive profiles

- **Quick:** a short bounded attempt to produce the first independently verified feasible result.
- **Balanced:** the default quality/latency profile. Start experimentally with approximately 2–3 seconds of CP-SAT time inside an approximately 3–5 second end-to-end hard interactive budget.
- **Deep:** an explicit longer budget, initially up to approximately 10 seconds for supported complex scenarios. It may improve a verified incumbent but never promises optimality.
- **Advanced:** allowlisted explicit settings with model-size, resource, reproducibility, and battery warnings. It cannot create an unbounded run.

The final values are versioned product policy derived from benchmark evidence. Backend, router, counterfactual, infeasibility-shrinking, transit fallback, and verification work consume one parent budget; nested operations do not each receive a fresh full budget.

### First useful result versus proof

A backend may find a strong feasible incumbent much earlier than it proves optimality. Eutheto therefore:

1. emits backend incumbents as untrusted candidates;
2. projects and independently verifies candidates before user-visible acceptance;
3. presents the best verified incumbent available within the selected budget;
4. continues bounded improvement only while policy and remaining budget allow;
5. records backend bound/proof/termination metadata for diagnostics; and
6. labels the final result `Optimal` only when the exact encoded objective has a valid proof and the projected candidate independently verifies.

A limit reached with a verified incumbent is a usable feasible result with accurate proof wording. A limit reached without one is `NoSolutionWithinLimit`, not `Infeasible`. A future **Optimize further** action may continue from a recorded result after the MVP, but it must create a separate run and never retroactively change an exported immutable result.

## Critical-path architecture

### Local-first solve inputs

The normal solve path consumes only the immutable local scenario revision and immutable, versioned enrichment snapshots already accepted for that revision. It must not synchronously perform a full calendar/provider synchronization merely because the user pressed **Optimize**.

Calendar and other provider synchronization:

- runs as an independently cancellable application operation;
- commits normalized changes transactionally before a solve captures its revision;
- records freshness, provenance, and stale/refresh status;
- preserves offline inspection and solving when embedded facts remain valid; and
- never changes the revision underneath an active solve.

If required enrichment is missing, the application may offer an explicit refresh action or a clearly disclosed bounded pre-solve enrichment step. It cannot hide provider latency inside solver time.

### Transportation lookup

Phase 14 owns provider-neutral immutable travel snapshots. Conceptually reusable entries are keyed by stable origin/destination, weekday or day class, departure-time bucket, transportation mode, provider/model version, and applicable confidence/freshness metadata.

The compiler and verifier make no network calls. A warm solve uses bounded local lookup. A cold enrichment operation fetches only the relevant pair/time buckets discovered from current scenario facts, never one remote request for every solver candidate. Licensing, attribution, retention, cache, and redistribution policy remains provider-specific and gates every adapter.

Strict no-transit-first solving remains semantic policy, not merely an optimization: Stage A uses normal transportation, and Stage B adds scheduled-transit candidates only when Stage A has no policy-sufficient verified plan and an affected person explicitly opted in. Both stages share one total budget. Unneeded transit matrices are neither fetched nor compiled.

### AI isolation

AI may propose structured inputs before a solve or paraphrase deterministic evidence afterward. The deterministic optimized result is never withheld while waiting for an LLM, and provider failure cannot prevent non-AI solve, verification, status, or deterministic explanation. AI time is measured separately and never included in a claim about core optimizer latency.

## Candidate-space management

Use deterministic filtering for easy impossibilities and reserve solver search for combinatorial choices. Before Planning IR, prune candidates proven impossible by:

- availability or hard time-window exclusion;
- resource/person incompatibility;
- location or travel-time impossibility;
- disabled transportation modes;
- hard eligibility or user restrictions;
- scenario-horizon exclusion; and
- dominance rules whose semantic equivalence is independently proven.

Record counts before and after each pruning class. Pruning must preserve every valid solution and every authoritative score possibility; exhaustive small-model differential tests compare pruned and unpruned semantics. Model-size estimates reject or require explicit review before constructing an input outside the supported envelope.

## Responsive solver UX

The webview remains interactive throughout validation, compilation, worker execution, verification, and result preparation. Rust/application jobs own blocking work; Vue receives bounded, throttled, versioned events.

For operations completing below the perceptual threshold, avoid flashing elaborate progress UI. At approximately 300–500 ms, show a stable progress region and cancellation using only evidence-backed coarse phases such as:

1. `Validating scenario`;
2. `Preparing candidate options`;
3. `Checking transportation data` when that phase actually runs;
4. `Building requirements and preferences`;
5. `Optimizing`;
6. `Verifying every required rule`; and
7. `Preparing results`.

Do not fabricate percentages, cycle fake steps, expose callback spam, or say `Found a valid plan` before independent verification. Progress announcements are coalesced for screen readers. Every active state has meaningful text, elapsed time where useful, cancel behavior, a hard timeout/resource outcome, and recovery guidance.

## Instrumentation and privacy

Every solve records bounded structured phase metrics, at minimum:

- `input_validation_ms`;
- `candidate_generation_ms`;
- `transport_lookup_ms`;
- `transport_network_ms` when an explicit enrichment operation ran;
- `domain_normalization_ms`;
- `planning_ir_build_ms`;
- `backend_translation_ms`;
- `worker_startup_ms`;
- `first_incumbent_ms`;
- `first_verified_feasible_ms`;
- `solver_ms`;
- `verification_ms`;
- `result_postprocess_ms`;
- `ui_render_ms`; and
- `total_ms`.

Also record bounded model/quality data: variables, constraints, objective terms/levels, candidates before/after pruning, worker count, seed, first/final backend objective and bound, authoritative score vector, proof/termination status, cache hits/misses, fallback/transit stage, peak memory where available, and version/hash provenance.

This is local diagnostic and benchmark evidence, not default telemetry. Metrics contain no names, notes, addresses, scenario documents, provider payloads, credentials, or arbitrary solver logs. Support/export inclusion remains previewable and redacted under the support-bundle policy.

## Representative benchmark packs

Maintain deterministic, versioned manifests and expected invariants for:

| Pack | Representative shape | Initial end-to-end objective |
|---|---|---:|
| Pack A — Small | 2–4 people/entities, several commitments, few locations/resources, basic preferences | **<500 ms warm** |
| Pack B — Common household/group or equivalent domain load | 4–8 people, dozens of events/options, multiple locations, shared resources, hard rules and preferences | **<1 s warm; usually <3 s cold** |
| Pack C — Moderately complex | Larger group, overlapping windows, transportation dependencies, significant preference optimization | **<5 s for the large majority** |
| Pack D — Difficult/stress | High branching, tight rules, fallback stages, large objective trade-offs | Responsive UI, fixed budget, accurate best-verified-result status |

Workforce and seating add domain-specific small/medium/large/stress fixtures in Phases 05, 07, and 09. Transportation adds equivalent manual, warm-snapshot, cold-enrichment, traffic-bucket, and transit-fallback fixtures in Phase 14. Phase 12 owns the cross-domain release corpus, fixed reference runner, raw artifacts, reviewed thresholds, and regression policy.

Each manifest records source/generator hash, fixed seed, schema/compiler/backend versions, scenario size, model-size bounds, warm/cold preconditions, expected status and required-rule invariants, authoritative score expectations/tolerances, budget, threads, and permitted variance. Personally identifying scenarios are never benchmark fixtures.

## Phase ownership and gates

| Phase | Required performance/UX delivery |
|---:|---|
| 01 | Parent resource-budget and timing DTOs; async job/persistence/file boundaries; no blocking database/import work on executor or webview threads. |
| 02 | One end-to-end budget contract; normalized termination/incumbent/proof semantics; model-size and pruning summaries; timing/progress records. |
| 03 | Worker startup/translation/first-incumbent instrumentation, bounded callbacks, process limits, and primitive benchmark evidence. |
| 04 | Independent-verification and explanation timing; first verified feasible rather than raw incumbent as the useful-result boundary. |
| 05 | Workforce candidate pruning, supported-size estimate, and deterministic small/typical/stress corpus. |
| 06 | Nonblocking desktop architecture, 300–500 ms progress threshold, event coalescing, cancellation, and UI long-task evidence. |
| 07 | End-to-end workforce target evidence, incumbent/proof UX, result rendering metrics, and responsive large-schedule flow. |
| 08 | Router/fallback comparison under one total budget; no unsupported backend is selected for speed alone. |
| 09 | Seating compile/solve/verify plus 500-guest canvas interaction evidence, with solver and rendering measured separately. |
| 10 | AI/provider latency outside the deterministic result path and separate measurement. |
| 11 | Exact-candidate benchmark summary and accurate public performance/limitation documentation. |
| 12 | Calibrated baseline, percentile/regression thresholds, supported-envelope decision, and release gate. |
| 14 | Bounded snapshot enrichment, warm/cold transportation benchmarks, Stage-A/Stage-B shared budget, and cache/freshness metrics. |

## MVP acceptance

The public MVP cannot claim this contract complete until evidence proves:

1. typical approved warm fixtures meet the calibrated interactive objective on the reference machine;
2. normal cold fixtures usually meet the calibrated cold objective and disclose network/enrichment time separately;
3. the webview remains responsive and keyboard/screen-reader cancellation works throughout every benchmark profile;
4. every solve, fallback, counterfactual, and diagnostic operation has a bounded parent budget;
5. a high-quality verified incumbent can be returned without waiting solely for proof of optimality;
6. status and wording distinguish proof, feasible incumbent, limit, cancellation, backend failure, verification failure, and proven infeasibility;
7. calendar synchronization and AI provider calls are absent from the ordinary critical path;
8. transportation uses reusable bounded snapshots and only introduces transit when policy requires and permits it;
9. detailed local phase/model/cache metrics exist without becoming default telemetry or leaking scenario data; and
10. benchmark regressions are reviewed continuously and block release when the supported envelope or UX target is violated.

## Post-MVP opportunities

After measured evidence identifies a need, later work may add incremental model reuse, solver hints/warm starts, background precomputation of likely travel pairs, adaptive budgets, proven dominance pruning, cached transit alternatives, transportation uncertainty ranges, opt-in extended optimization, comparison solves reusing prior state, or deterministic search-order heuristics. None may weaken revision identity, independent verification, score authority, resource bounds, privacy, or reproducibility.

Planner-evolution operations—ranked repair search, arbitrary comparison, flexibility probes, trade-off portfolios, and stress tests—can multiply solver work and therefore require their own measured interaction envelopes before release. One visible parent operation owns total wall-clock, CPU/thread, memory, event/log, and retained-result limits across all children; cancellation stops undispatched work and terminates active work under the normal grace policy; an already accepted result remains available; and partial or inconclusive analysis is reported honestly. Deterministic pack-defined perturbations precede sampled resilience analysis. Probability or confidence claims require reviewed source distributions, calibration evidence, sample/error disclosure, reproducible seeds/manifests, and performance evidence on representative hardware.