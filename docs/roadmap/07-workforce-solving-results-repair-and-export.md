# Phase 07 — Workforce Solving, Results, Repair, and Export

## Outcome

Deliver the complete workforce-scheduling MVP vertical slice: every official required rule and preference compiles, solves through the default OR-Tools path, is independently verified and explained, appears in accessible result and repair workflows, and round-trips through the supported imports and exports. Every published or exported solution is an accepted, revision-matched, independently verified solution.

This phase completes the behavior begun in Phases 05–06. It does not weaken the separation between domain meaning, planning IR, backend translation, and independent verification.

## Source coverage

This phase is the implementation source of truth for blueprint Sections 17–18; workforce solve/result portions of Section 22; Phases 7 and the applicable Phase 5–6 prerequisites; Sections 31.2–31.4, 32, and 33.1–33.4; CLI Appendix C; Tauri API Appendix D; and backlog items `WF-004` through `WF-008`, `UI-005`, `UI-006`, `EXPLAIN-001`, `REPAIR-001`, and `EXPORT-001` from Appendix I.

Project-wide contracts and sequencing are in [README.md](README.md). Version and evidence gates are in [assumptions.md](assumptions.md). This phase depends on [Phase 02](02-domain-pack-and-planning-ir-contracts.md), [Phase 03](03-ortools-worker-vertical-slice.md), [Phase 04](04-independent-verifier-and-explanations.md), [Phase 05](05-workforce-core-vertical-slice.md), and [Phase 06](06-desktop-design-system-and-workforce-setup.md). Backend diversification follows in [Phase 08](08-pumpkin-backend-and-router.md); seating reuses these result/repair contracts in [Phase 09](09-seating-domain-and-venue-experience.md); AI may invoke only the same typed commands in [Phase 10](10-ai-assistant-mvp.md).

## Dependencies and entry conditions

- Workforce entities, time-zone/DST semantics, shift generation, stable IDs, migrations, eligibility, availability, coverage, no-overlap, and minimum-rest foundations exist from Phase 05.
- Revisioned command batches, undo/redo, scenario persistence, planning IR, provenance, score vectors, canonical model hashes, solution projection, and verifier interfaces are stable enough to extend without a second convention.
- The OR-Tools worker protocol, process supervision, cancellation, resource limits, status normalization, and candidate projection work end to end.
- Phase 04 rejects deliberately invalid candidates and persists deterministic verification/explanation evidence.
- The Phase 06 desktop shell, rule builder, validation UX, import preview, generated Rust-to-TypeScript API boundary, keyboard/focus infrastructure, and semantic design tokens exist.
- The working CLI executable name `optimizer` and `.optplan` bundle extension are examples only until their explicit naming gates close; code and documentation must use the final decisions consistently when selected. The project/package prefix is `eutheto`.

## Decisions and invariants

1. **Required means acceptance-critical.** A required rule violation always rejects a candidate. A preference is a bounded objective contribution and can never silently change required feasibility.
2. **Verification is independent.** Backend values are projected into a normalized domain solution, structurally validated, evaluated against every original domain required rule, and rescored independently. Backend status and compiler penalty variables are not authoritative.
3. **Verification failure is quarantined.** Store the raw result only in bounded, redacted diagnostics; fail the run; show an internal-correctness error; disable select, publish, repair-from, and assignment/result/solution-derived export actions for the candidate. Project/scenario backup bundles and diagnostic/infeasibility reports remain available when their own source data is valid and must not imply candidate acceptance.
4. **Revision identity is explicit.** Jobs, events, solutions, verification reports, explanations, exports, and counterfactuals carry scenario revision and relevant model/solution hashes. An older-revision result remains inspectable but is visibly stale and cannot masquerade as current.
5. **Time is unambiguous.** Rest, overlap, and actual-hours rules operate on elapsed instants unless a rule explicitly names scheduled/local hours. Recurring generation applies the scenario’s reviewed DST policy and reports wall-clock versus elapsed differences.
6. **Fairness is configured, not asserted.** Show the chosen population, workload bucket, target/share computation, actual distribution, deviations, and spread. Never present one opaque “fairness percentage.”
7. **Repair semantics are explicit.** Hard locks cannot change; soft-lock changes incur a high stability penalty; unlocked assignments can change. Search hints never replace locks or stability objectives.
8. **No exact-schedule assertions by default.** Equivalent valid schedules are expected. Tests assert status, verification, score invariants, metrics, and stable explanation keys unless deterministic settings intentionally select one arrangement.
9. **Solution-derived exports use accepted solutions only.** Assignment/result/solution-derived export preview identifies scenario/solution revision, verification checksum, included data, and stale status. A stale but previously accepted result may be exported only after an explicit warning and accurate labeling; an unverified or rejected candidate never may drive those exports. Project/scenario backup bundles and diagnostic/infeasibility reports have separate input-validity gates and require no accepted solution.
10. **Human language is primary.** Use `Required`, `Preference`, `Optimize`, and `Repair plan`; backend jargon is confined to advanced diagnostics.

## Complete workforce scope

### Domain data completed or consumed

`WorkforceScenario` contains planning horizon, calendars, people, qualifications, locations, assignment types, shift templates and instances, coverage requirements, availability, required rules, preferences, optional base schedule, assignment locks, and the workforce score policy.

The solve/result implementation must preserve these facts:

- People use stable IDs, never names as identity keys; include display name, active range, qualifications/eligibilities, eligible assignment types, optional home location, optional contract/workload target, tags/teams, and display-only color/avatar metadata.
- Qualifications are dated capabilities, including effective and expiration dates.
- Assignment types define semantic category, time behavior, default duration, eligible qualifications, location behavior, and configured workload-bucket counting.
- Shift instances retain template ID when generated, assignment type, location, zoned start/end, required/preferred/maximum counts, required qualification sets, and tags.
- Detached one-off instances survive regeneration; regeneration shows a diff and never overwrites manual edits without confirmation.
- Availability supports unavailable intervals, weekly recurrence, available-only windows, approved leave, effective ranges, assignment-type restrictions, location restrictions, source, and note; normalized rejection provenance remains inspectable.

### Exhaustive required-rule catalog

Each rule below requires a stable schema/ID, migration handling, command DTO, validation, sentence-like editor, planning-IR compilation and backend tests, independent evaluator, provenance/explanation, CLI/document example, boundary and infeasible fixtures, import/export preservation, model-size review, and typed AI exposure only when appropriate.

1. **Eligibility:** assign only when every required qualification and assignment-type eligibility condition holds, including effective/expiration dates.
2. **Availability:** reject a shift intersecting an unavailable period or outside an available-only window, including recurring, approved-leave, type, location, and effective-range filters.
3. **Coverage:** meet exact, minimum, and bounded maximum staffing for every shift and every required qualification slot.
4. **No overlapping assignments:** one person cannot hold overlapping shifts unless explicit configuration declares the two categories compatible and defines counted hours.
5. **Minimum rest:** require elapsed duration between a matching source assignment’s end and target assignment’s start; scope may filter source/target assignment types, people, teams, weekdays, or locations. The reference fixture enforces 10 hours after overnight call before clinic.
6. **Maximum hours in a fixed period:** cap configured counted hours per day, week, pay period, or custom calendar interval.
7. **Maximum hours in a rolling window:** cap counted hours in every rolling duration, such as 80 hours in any 14-day window. Generate candidate windows only where a shift start/end changes the sum, and enforce model-size limits.
8. **Consecutive assignments/days:** cap consecutive worked days, nights, or matching assignment types.
9. **Maximum assignment count:** cap matching assignment counts in a configured period.
10. **Required skill mix:** require at least the configured number of assigned people satisfying each qualification expression.
11. **Fixed assignment and hard lock:** force the named assignment and preserve hard-locked base work during repair.
12. **Mutual assignment restriction:** keep configured people apart on the same shift or on overlapping matching shifts where this is a legitimate operational rule.
13. **Transition/travel time:** require configured setup/travel duration between assignments at distinct locations using the MVP location-pair duration matrix; no map service is needed.

Empty or default scopes must be safe. An empty scope cannot be saved accidentally; an intentional inactive state is explicit and never compiles as a surprising global rule.

### Exhaustive preference catalog

Every preference has a priority and bounded weight. The visible labels `Low`, `Normal`, `High`, and `Very high` map through the persisted score policy; numeric backend coefficients are not the user contract.

1. Prefer or avoid a person-specific date/time.
2. Prefer or avoid an assignment type.
3. Prefer a location.
4. Honor requested time off when it is not Required.
5. Keep assignment count near a target.
6. Balance a named workload bucket across a peer group.
7. Avoid consecutive nights or weekends.
8. Prefer or avoid assignments adjacent to existing work.
9. Keep a base schedule stable.
10. Keep configured people together or separate when operationally appropriate.
11. Prefer coverage above the required minimum.

### Fairness and score policy

The setup and result views allow the user to balance any explicit combination of:

- assignment count;
- counted hours;
- overnight call;
- weekends;
- holidays;
- undesirable shifts;
- deviation from each person’s target or FTE-weighted share.

Use lexicographic/priority score levels so required feasibility is outside the preference score, high-level stability can dominate ordinary preferences during repair, and all contributions are bounded. The recommended default fairness penalty is convex target deviation via absolute deviation or reviewed piecewise penalties. Unequal contracts use configurable workload weights; equal raw shares occur only when explicitly selected. Persist score-policy/profile IDs and complete objective configuration.

Named alternatives map to recorded objective profiles, not labels alone:

- `Balanced`;
- `Preferences first`/`Preference-focused`;
- `Minimal changes`;
- `Fairness first`.

Run profiles sequentially in the MVP, verify every candidate, compare authoritative score vectors and visual diffs, and suppress alternatives that differ only through arbitrary tie-breaking without meaningful metric change.

### Compilation and model construction

For each statically feasible person/shift pair, create `x[person, shift] ∈ {0,1}`. Do not allocate variables for impossible pairs after eligibility/static-availability preprocessing, but retain rejection provenance for explanations.

Core formulations include:

- coverage: `min ≤ Σ x[p,s] ≤ max`;
- person overlap/rest conflicts: `x[p,s1] + x[p,s2] ≤ 1`;
- counted-hour limits: `Σ duration_bucket[s] × x[p,s] ≤ limit`;
- skill mix sums over only people satisfying the qualification expression;
- preferences compile to bounded penalty terms and never alter required coverage.

Benchmark optional intervals/no-overlap against pairwise conflict cliques and cardinality constraints for fixed shifts. Apply symmetry breaking only where person distinctions are truly interchangeable; qualifications, preferences, teams, and identity-visible effects prevent arbitrary ordering.

### Validation

Fast validation after edits covers:

- unique IDs and names where required;
- valid time ranges;
- existing references;
- non-negative counts and hours;
- non-empty or explicitly inactive rule scopes;
- templates generating within the horizon;
- existing qualifications referenced by coverage.

Full pre-solve validation additionally covers:

- enough statically eligible candidates for every required shift;
- internally possible qualification coverage slots;
- individually valid hard locks;
- overlapping hard locks;
- obvious minimum-rest conflicts among hard locks;
- hours rules already violated by locks;
- non-empty target/fairness groups;
- rolling-window generation within configured model/resource limits.

Validation issues are grouped as `Must fix before optimizing`, `Likely problem`, `Review suggested`, and `Information`; each uses plain language, names affected entities, deep-links to the editor/field, offers only deterministic safe bulk fixes, and distinguishes malformed/incomplete data from proven optimization infeasibility.

## Solve jobs and result experience

### Optimize flow

Before `Optimize`, show scenario revision and validation status, Quick/Balanced/Deep mode, optional maximum time, repair/base-plan state, and backend only in advanced disclosure. Quick is short bounded search for a verified feasible plan; Balanced is the default budget/quality profile; Deep uses a longer search and may seek more alternatives but never promises optimality.

Emit only supported, throttled progress states: queued; compiling with phase/optional percent; backend started; presolve summary; incumbent found; bound improved; safe bounded log line; verifying; explaining; completed. User-facing copy may say `Preparing the problem`, `Checking … possible assignments`, `Optimizing with OR-Tools CP-SAT`, `Found a valid plan`, `Improving preferences and fairness`, and `Verifying every required rule` only when the corresponding event proves it. Include job ID, scenario ID/revision, event version, and timestamp on every event. Provide cancellation and allow navigation. A same-scenario edit either cancels the job or allows it to finish against its recorded revision and labels the result stale.

Normalize results as `Optimal`, `Feasible`, `Infeasible`, `Unbounded`, `NoSolutionWithinLimit`, `Cancelled`, `InvalidModel`, `BackendUnavailable`, or `BackendFailed`. Map backend `Unknown` from incumbent presence and termination cause. Never translate feasible into optimal.

### Independent verification and scoring

Acceptance pipeline:

```text
backend values
→ normalized workforce solution projection
→ structural solution validation
→ independent evaluation of every required workforce rule
→ independent score recomputation
→ accepted solution
```

`VerificationReport` records acceptance, evaluated revision, every required `RuleEvaluation`, authoritative `ScoreVector`, warnings, typed metrics, and checksum. A rule evaluation names affected entities, relevant time facts, expected and observed conditions, and a localized stable message key. Shared time/interval/value-object utilities are allowed; re-querying the backend model, trusting backend status, or using compiler penalty variables as authoritative scores is prohibited. Differential mutation tests must prove compiler/verifier disagreement is detected.

### Result views

Implement virtualized views rather than a DOM cell per large horizon:

- schedule/calendar grid by person;
- schedule grid by shift and location;
- person detail timeline;
- uncovered and overcovered work;
- required-rule summary;
- preference-satisfaction summary;
- fairness distribution with targets, actual values, deviation, and spread;
- changes from base schedule;
- warnings and accurate solver/proof status;
- explanation side panel.

The result header states an accurate `Verified schedule ready` alternative, that all required rules passed, optimal/feasible/limit status, secondary time/backend details, preference/fairness metrics, base-plan changes, and warnings. Design normal, empty, loading, stale, active, cancelled, backend-unavailable, no-solution, older-revision, and internal-verification-failure states. No indefinite spinner lacks text, cancellation, or timeout behavior.

### Explanation catalog

Persist and render all seven explanation categories:

1. validation: incomplete or contradictory scenario before solve;
2. infeasibility: sufficient subset of Required rules that cannot coexist;
3. assignment: verified eligibility/availability facts and preference/score effects;
4. counterfactual: effects of temporarily forcing or forbidding an assignment;
5. solution difference: changed assignments and score/proof differences;
6. repair: why a prior assignment changed after new facts;
7. optimality/status: proof versus bounded incumbent.

An assignment inspector shows assigned person/work, eligibility, availability, rule checks, preferences helped/hurt, fairness/stability contribution, lock state, and actions `Why this?`, `Why not…?`, and `Try a change`.

Why-assigned statements are limited to verified facts: eligibility; unavailable/incompatible competing assignments; named preference satisfaction/violation; calculated fairness/stability deltas. `Why not Alice instead?` clones the same revision, adds one temporary force/forbid condition, uses a bounded diagnostic budget, verifies any candidate, and returns impossible, worse by named score categories, or undistinguished within budget. Never claim unique causality across equivalent optima.

For infeasibility: confirm no verified candidate; read an available assumption core; map assumptions through provenance; bounded-shrink an oversized core by removing a rule/group and re-solving within a small diagnostic budget; retain removal only while infeasibility remains proven; stop on budget exhaustion; render pack-owned evidence and label it `sufficient conflict`, never `minimal` unless proved. Offer inspect, edit, temporary relaxation in a reversible diagnostic copy, or export diagnostic report. Never alter the original.

Comparison computes added/removed/changed assignments, required-rule status, score-vector deltas by level/category, fairness deltas, preference deltas, affected people/groups, lock preservation, and solver status/proof differences.

Persist compact assumption-to-rule mappings, objective contribution records, counterfactual summaries, backend bounds/status, verifier evaluations, and model/solution hashes. Full solver logs are opt-in and bounded. A later AI paraphrase never replaces the deterministic evidence view.

## Repair and manual edits

A base assignment is `Hard locked`, `Soft locked` (`try to keep`), or `Unlocked`. Compile a change indicator for every base assignment and minimize changes at a high objective level. A previous solution may be a search hint, but correctness comes from explicit locks and stability penalties.

Before committing an invalid drag/reassignment, explain the violated rule and offer cancel, edit a related rule, convert the request to a preference and re-optimize, replace a conflicting assignment, or create a diagnostic copy. A valid manual change is a revisioned, undoable command and may remain unlocked, soft locked, or hard locked. `Repair plan` becomes primary after edits and defaults to minimum change.

The repair diff highlights unchanged, moved/replaced, newly unfilled, and newly assigned work, plus the new rule or fact requiring each explainable change. Hard locks must survive; any impossible hard-lock set is rejected before solve with actionable evidence.

## Imports and exports

### Imports

Support people CSV, eligibility CSV or pasted matrix, shift-instance CSV, availability/time-off CSV, and existing-assignment CSV. Every importer follows choose file, safe format/encoding detection, column mapping/bundle summary, additions/updates/duplicates/rejected-row preview, stable-identity matching, proposed-change validation, one atomic undoable command batch, and a report with downloadable rejected rows. Never partially mutate or merge similar names silently.

### Exports

Support all MVP outputs:

- project/scenario bundle using the extension selected by the unresolved extension gate (working blueprint extension `.optplan`);
- assignment CSV;
- people/shift summary CSV;
- iCalendar `.ics`, per person or combined;
- locally produced print-friendly HTML;
- JSON through the CLI;
- bounded, redacted diagnostic/infeasibility report.

Assignment CSV, people/shift summaries derived from a result, ICS, print views, and solution-bearing CLI JSON require an accepted independently verified solution. Project/scenario backup bundles remain available whenever the project/scenario can be validly serialized, including before any solve or after infeasibility; they accurately label any included solution status and never elevate an unverified candidate. Diagnostic/infeasibility reports remain exportable whenever their bounded, redacted diagnostic inputs are valid, including when no accepted solution exists, and must identify the scenario revision, run/status, proof limits, and whether infeasibility was proved.

Export UX starts from the goal: back up/share project; publish assignments; print/present; spreadsheet analysis; or another application. Preview explains whether rules, accepted solutions, history, or assignments are included. CSV and ICS schemas are deterministic, stable-ID-aware, time-zone explicit, escaped correctly, and tested for round trip or consumer compatibility. Print HTML is local, self-contained under the export policy, color-independent, and carries verification/status metadata.

XLSX, PDF, payroll/vendor adapters, and live calendar synchronization are post-MVP and not implied by these outputs.

## Application and CLI contracts

### Tauri API

Use revisioned request/response envelopes. Mutations carry `scenario_id`, `expected_revision`, and `request_id`; responses carry request ID, current revision, warnings, and schema version. Errors are typed/actionable and never expose Rust backtraces.

Required solve endpoints: `solve_get_backend_options`, `solve_estimate_model`, `solve_start`, `solve_cancel`, `solve_get_job`, `solve_list_runs`, and `solve_get_diagnostics_summary`.

Required solution endpoints: `solution_list`, `solution_get_summary`, `solution_get_view`, `solution_select`, `solution_verify`, `solution_compare`, `solution_explain`, `solution_start_counterfactual`, `solution_cancel_counterfactual`, `solution_lock_assignment`, `solution_unlock_assignment`, `solution_create_repair_request`, `solution_export_preview`, and `solution_export`. Lock convenience endpoints issue normal scenario commands.

Events: `solve://progress`, `solve://completed`, `scenario://changed`, `scenario://validation-changed`, `counterfactual://progress`, and application notifications. Only typed frontend API modules invoke Tauri directly.

### CLI

The working command form is `optimizer` pending the CLI naming gate. It runs without desktop state, uses the same application/domain/backend/verifier services, supports human and JSON output, sends diagnostics separately, never prints secrets, accepts explicit paths, and preserves stable exit codes.

Relevant commands are `scenario validate`, `scenario apply`, `solve`, `solutions list`, `solutions verify`, `solutions compare`, `solutions explain`, and `solutions export`. Solve options are backend override, quick/balanced/deep mode, max time, threads, seed, first feasible, repair-from solution, output, bounded diagnostics directory, and human/JSONL/none progress. Load/migrate, validate, compile/route, solve, project/verify, write only an accepted normalized solution, then emit status/proof/score.

Exit behavior: `0` produced the requested result; `3` validation failure; `4` proven infeasible; `5` no verified result in limits; `6` backend unavailable/incompatible/failed; `7` candidate verification/correctness alarm; `8` file/bundle/database/migration failure; `10` revision/state conflict; `130` user cancellation. Proven infeasibility is a completed domain outcome, not an internal crash.

The JSON envelope uses the `eutheto` API namespace, command, `ok`, normalized status, result, warnings, and optional diagnostic ID. The exact CLI schema version is pinned with compatibility fixtures.

## Ordered work packages

1. **WF-004 — Complete required rules:** implement hours, rolling windows, consecutive, maximum count, skill mix, locks, mutual restrictions, and travel transitions through schema-to-explanation slices.
2. **WF-005 — Complete preferences/fairness/stability:** bounded score contributions, FTE/target policy, named profiles, score serialization, metric views, and small-model differential checks.
3. **WF-006/WF-007 — Complete compiler and independent verifier:** preprocessing provenance, formulations, rejection paths, all rule evaluators, authoritative score, mutation tests, and verification quarantine.
4. **UI-005 — Solve orchestration:** revisioned job service, progress throttling, cancellation, stale-result handling, normalized status/error UI, and advanced disclosure.
5. **UI-006 — Results:** virtualized grids/timelines, coverage, preference, fairness, base-diff, result summary, empty/error states, keyboard and screen-reader paths.
6. **EXPLAIN-001 — Explanation and diagnostics:** assignment evidence, bounded counterfactual jobs, infeasibility mapping/shrinking, deterministic comparisons, and compact persistence.
7. **REPAIR-001 — Locks and repair:** base-solution references, lock commands, high-level stability objective, manual-edit validation, repair diff, one-step undo.
8. **Alternatives:** sequential recorded objective profiles, verified metrics, semantic deduplication, comparison UI.
9. **EXPORT-001 — Export suite:** separate input-validity gates; accepted-solution gate for assignment/result/solution-derived outputs; no-solution backup and diagnostic/infeasibility exports; preview; assignments/summary CSV; ICS; local print HTML; CLI JSON; deterministic fixtures.
10. **Acceptance hardening:** all workforce fixtures, large-grid profiling, task-based usability scripts, docs and generated contracts.

## Tests and acceptance fixtures

Checked-in minimum corpus:

1. **Clinic only, small:** 8 people, 4 weeks, availability and fairness.
2. **Clinic plus overnight call:** 12 people and cross-category 10-hour rest.
3. **Rolling-hours stress:** overlapping candidate windows and hard locks.
4. **Qualification coverage:** one specialist required per session.
5. **Repair after call-out:** accepted/published base schedule plus one new unavailability.
6. **Provably infeasible:** insufficient eligible coverage with mapped sufficient conflict.
7. **DST transition:** overnight work across both spring and fall transitions.
8. **Large benchmark:** configurable 100+ people and thousands of candidate assignments.

For every fixture assert domain validation, expected normalized status, independent verification, required-rule outcomes, score invariants, stable explanation keys, import/export preservation, and revision/hash identity. Add valid, invalid, edge, empty-scope, period-boundary, lock-conflict, cancellation, stale-result, and resource-limit cases per rule. Differential exhaustive small models compare compiler results with the independent evaluator; intentional mutation proves detection.
Export tests separately prove that unverified/rejected/no-solution runs cannot produce assignment/result/solution-derived outputs, while a valid project/scenario backup and a valid bounded diagnostic/infeasibility report remain exportable before any accepted solution and after proven infeasibility.

Desktop acceptance includes keyboard-only setup/result/lock/repair/export, focus restoration, semantic grid headers, screen-reader solve completion and validation announcements, no color-only state, cancellation, stale and internal-failure states, and a representative large grid that does not freeze the webview.

Required usability tasks: create a small schedule, import people, express one required rest rule and one preference, understand an infeasible conflict, lock and repair, inspect `Why this?` and `Why not?`, and export. Repeated confusion about Required versus Preference or solver jargon blocks exit.

## Risks and failure handling

- **Compiler/verifier disagreement:** quarantine and correctness alarm; separate evaluators, exhaustive differential/mutation tests.
- **Feasible mistaken for optimal:** normalized proof state and explicit time-limit language.
- **Large/nonminimal conflict:** bounded shrinking and `sufficient conflict` wording.
- **Rule/model explosion:** static pair filtering, boundary-based rolling windows, conflict cliques/global constraints, model estimate, resource limits, benchmarks.
- **DST error:** IANA time zone, instant arithmetic, explicit ambiguity/nonexistence policy, spring/fall fixtures.
- **Misleading fairness:** explicit target population and raw metrics; no universal fairness score.
- **Stale solve/export:** revisioned jobs and warnings; reject unverified assignment/result/solution-derived exports without blocking valid project/scenario backups or diagnostic/infeasibility reports.
- **Backend crash/unavailability:** preserve scenario, typed retry/recovery; a fallback is governed by Phase 08, not improvised here.
- **Webview overload:** purpose-built queries, virtualization, throttled events, no solver/webview-thread work.
- **Import corruption:** bounded parsers, preview, stable-ID disambiguation, atomic command batch.
- **Compliance assumptions:** verified means configured rules passed, not legal or professional compliance.

Pause and write an ADR if a rule cannot be independently verified, a backend needs workforce-specific knowledge, the UI needs database access, decomposition cannot prove independence, migration loses scenarios, or correctness depends on undocumented backend behavior.

## Exit gate

Phase 07 is complete only when:

- all thirteen Required rules and all eleven preference categories satisfy the rule checklist and domain-rule definition of done;
- every workforce fixture passes validation, expected status, independent verification, scoring, explanations, and accepted-solution export checks;
- repair preserves every hard lock and minimizes changes according to the recorded score policy;
- results accurately distinguish optimal, feasible, infeasible, cancelled, and time/resource-limited outcomes;
- assignment and counterfactual explanations return impossible, worse by explicit category, or undetermined—never invented causality;
- every assignment/result/solution-derived export comes from an accepted verified solution; project/scenario backup bundles and diagnostic/infeasibility reports remain available without one when their own inputs are valid; and all supported import/export flows are atomic and reviewed;
- large benchmark solve/progress/results do not freeze the webview and virtualization is profiled;
- keyboard, screen-reader, focus, non-color, stale/error/offline-equivalent core states, revision conflict, undo/redo, and representative-user usability gates pass;
- CLI and desktop call the same services and the desktop never owns authoritative scenario state.

## Deferred and non-goals

- Pumpkin selection and multi-backend routing are Phase 08; OR-Tools remains the default here.
- XLSX, PDF, payroll/vendor adapters, and live calendar synchronization are post-MVP.
- Concurrent portfolios, superficial alternative generation, map-service travel calculation, legal-compliance certification, arbitrary solver parameters, and full unbounded solver logs are excluded.
- AI is not required to create, solve, verify, explain deterministically, repair, import, or export a workforce project.

## Assumption and version gates

- Use the Phase 03 pinned OR-Tools/protobuf pair and record worker, adapter, model, seed, thread count, and options. Do not independently upgrade protocol dependencies here.
- Exact lockfile pins are established in Phase 00; frontend result work uses the verified 2026-08-29 stack recorded in [assumptions.md](assumptions.md), including Vue 3.5.42, Vue Router 5.3.0, Pinia 4.0.3, TanStack Table 9.2.4, and TanStack Virtual 3.13.36. Account for Table v9 API and Pinia 4 ESM/devtools requirements.
- The CLI name, bundle extension, application ID, and release/hosting decisions remain explicit gates; examples are not decisions.
- Before public release, practitioner review must confirm workforce defaults, fairness presets/weights, workload bucket semantics, and any medical-practice wording. Presets remain starting templates, not compliance claims.
