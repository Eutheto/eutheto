# Phase 03 — OR-Tools worker vertical slice

## Outcome

Deliver the first complete solver-backed path in `eutheto`: validated solver-neutral planning IR is translated to CP-SAT, sent to an isolated project-owned C++ worker, solved under explicit limits, returned through a bounded versioned protocol, projected by Rust, and handed to the independent verification boundary. The same worker source must build under Nix and the supported native Windows toolchain and must launch as the exact Tauri sidecar packaged for every MVP target.

This phase establishes backend mechanics, not trust in backend output. A worker result is only a candidate. Phase 04 independently verifies the projected domain solution and computes its authoritative score.

## Source coverage

This phase incorporates blueprint Sections 14 and 15; Section 25.11; worker packaging requirements from Sections 27.1–27.4, 27.8–27.10; Phase 3; Appendix E; worker-relevant material from Appendices B, H, I, J, and K; and the worker-owned measurements in [Performance and Solver UX Targets](performance-and-solver-ux-targets.md). It depends on the contracts in [Phase 02](02-domain-pack-and-planning-ir-contracts.md), hands candidate verification and explanations to [Phase 04](04-independent-verifier-and-explanations.md), and provides the backend used by [Phase 05](05-workforce-core-vertical-slice.md). Public signing and release publication remain in [Phase 11](11-public-mvp-packaging-and-documentation.md).

## Dependencies

- Phase 00 provides the Cargo workspace, CMake/Ninja/Nix tooling, exact lockfiles, protobuf generation, CI target matrix, `xtask`, and legal-policy checks.
- Phase 01 provides typed IDs/errors, process-launch abstraction, job lifecycle, cancellation tokens, redacted tracing, resource-budget types, and application services.
- Phase 02 provides immutable validated `PlanningProblem`, capabilities, projections, provenance, score plan, canonical hash, and backend-neutral constraint records.
- Rust code uses `tokio`/`tokio-util` for async process supervision and cancellation, `prost`/`prost-build` for the project protocol and pinned upstream protos, `semver` for compatibility metadata, `serde` for manifests, and structured `tracing`. Blocking process I/O must not occupy async executor threads.
- OR-Tools and its protobuf definitions are one inseparable source pin. `protoc`/protobuf versions must match the selected OR-Tools 9.15 contract; the project must not force the newest protobuf 36.0 when that breaks generated-wire or build compatibility.

## Decisions and invariants

### Trust and dependency boundaries

- `SolverBackend` consumes only planning IR; backend crates do not depend on official domain crates.
- The C++ worker contains no workforce, seating, or other domain logic. It validates `CpModelProto`, applies an allowlisted parameter set, runs CP-SAT, and returns bounded values and evidence.
- Backend status is never a domain-rule result. Rust projects returned values, Phase 04 evaluates original domain semantics, and only a verified solution can be accepted, exported, or published.
- No C++ ABI crosses into `eutheto` core. Worker crashes, native memory failures, malformed output, and cancellation are isolated to a child process.
- The bundled default solver is resolved from the application bundle, never from `PATH`. Any future user-provided backend uses a separate explicit configuration and trust flow.
- Official artifacts are Apache-2.0 compatible. Disable GLPK, proprietary integrations, language wrappers, examples, and unrelated solver components. Inspect the exact linked artifact rather than trusting configuration intent.
- The worker receives only the remaining parent solve budget after validation/compilation. Startup, translation, fallback, callback handling, and worker execution cannot each reset the full interactive limit.

### Backend contract

```rust
#[async_trait]
pub trait SolverBackend: Send + Sync {
    fn descriptor(&self) -> &SolverDescriptor;

    fn compatibility(
        &self,
        problem: &PlanningProblemSummary,
        options: &SolveOptions,
    ) -> CompatibilityReport;

    async fn solve(
        &self,
        problem: Arc<PlanningProblem>,
        options: SolveOptions,
        progress: ProgressSink,
        cancellation: CancellationToken,
    ) -> Result<BackendSolveResult, BackendError>;
}
```

```rust
pub struct SolverDescriptor {
    pub id: SolverId,
    pub display_name: String,
    pub version: String,
    pub adapter_version: String,
    pub distribution: SolverDistribution,
    pub license: LicenseMetadata,
    pub stability: BackendStability,
    pub capabilities: SolverCapabilities,
}
```

`SolverDistribution` includes `BuiltIn`, `BundledWorker`, and `UserProvided`; `BackendStability` includes `Stable`, `Beta`, and `Experimental`. OR-Tools is a `BundledWorker`. Its descriptor and release manifest expose worker, OR-Tools, adapter, protocol, capability, distribution, stability, and license metadata.

Compatibility is checked before launch and is explainable:

```rust
pub struct CompatibilityReport {
    pub compatible: bool,
    pub unsupported_features: Vec<UnsupportedFeature>,
    pub warnings: Vec<CompatibilityWarning>,
    pub estimated_translation_cost: Option<ModelCostEstimate>,
}
```

A desktop backend picker or CLI request must reject an incompatible override with exact unsupported features. Known unsupported IR must never be discovered only after solving starts.

### Solve options, status, and deterministic records

```rust
pub struct SolveOptions {
    pub mode: SolveMode,
    pub max_time: Duration,
    pub worker_threads: Option<u16>,
    pub random_seed: u64,
    pub backend_override: Option<SolverId>,
    pub stop_after_first_feasible: bool,
    pub collect_intermediate_solutions: bool,
    pub explanation_mode: ExplanationMode,
    pub resource_limits: ResourceLimits,
}
```

User modes map to recorded settings:

- **Quick:** short bounded search; return a verified feasible solution when one is found.
- **Balanced:** default budget and quality policy.
- **Deep:** longer search, stronger presolve/portfolio settings, and more alternatives; it does not promise optimality.
- **Advanced:** explicit backend and detailed allowlisted settings with warnings and reset-to-safe-default behavior.

Normalize every terminal condition:

```rust
pub enum SolveStatus {
    Optimal,
    Feasible,
    Infeasible,
    Unbounded,
    NoSolutionWithinLimit,
    Cancelled,
    InvalidModel,
    BackendUnavailable,
    BackendFailed,
}
```

Backend `UNKNOWN` is not exposed as a lazy catch-all. Map it using whether a candidate exists, whether that candidate verifies, and the actual termination reason. Never call `Feasible` optimal. Record seed, worker count, backend/worker/OR-Tools/adapter/protocol versions, model hash, applied options and parameter hash. Tests use fixed seeds and deterministic single-worker settings where available; production may use multiple workers and may find different equally valid solutions.

### Progress lifecycle

```rust
pub enum SolveProgressEvent {
    Queued,
    Compiling { phase: String, percent: Option<f32> },
    BackendStarted { backend: SolverId },
    PresolveSummary(ModelReductionSummary),
    IncumbentFound(IncumbentSummary),
    BoundImproved(BoundSummary),
    LogLine(SafeDiagnosticLine),
    Verifying,
    Explaining,
    Completed(SolveCompletionSummary),
}
```

Only emit statements backed by observed state. Throttle/coalesce callbacks before application events reach a webview; never forward thousands of solver callbacks per second. Every downstream event carries job/request ID, scenario revision, event schema version, and timestamp so stale listeners can ignore it.

### Router and fallback behavior

The router is deterministic policy:

1. Honor an explicit compatible backend override.
2. Otherwise choose OR-Tools CP-SAT for supported nontrivial official-domain models.
3. Choose a native specialized algorithm only for a compiler-recognized exact structure with an available verifier.
4. Never auto-select experimental Pumpkin in the public MVP unless experimental backends are enabled and its support matrix has no gaps.
5. Split connected components only after hypergraph proof: no constraint, objective term, global score normalization, projection, or domain invariant spans components; merged values must remain domain-valid. Fairness often joins otherwise separate assignments.
6. Persist the routing choice and reasons.

Fallback is explicit and shares the total budget:

- unavailable/crashed-before-candidate: a configured compatible fallback may run;
- invalid model: report compiler/adapter defect and retain diagnostics; no silent fallback;
- proven infeasible: do not treat as a performance failure; a cross-check is a separate advanced action;
- limit without candidate: fallback may run only within remaining total budget;
- failed independent verification: quarantine the candidate as a critical defect; another backend may run only after that failure is recorded.

Concurrent portfolio execution and cross-solver decomposition are post-MVP. A future portfolio must share one resource budget, independently verify every candidate, compare authoritative score vectors, safely cancel losers, preserve candidate provenance, and avoid multiple heavy solvers by default on battery-constrained systems.

## Worker protocol and process supervisor

### Process lifecycle

Use one fresh worker per solve:

1. Resolve the bundled executable for the exact target triple.
2. At application startup, validate executable location, expected manifest, hash, target, backend, versions, and capabilities; repeat before use whenever the resolved path changes.
3. Launch through the process abstraction with a sanitized environment and private temporary working directory.
4. Complete handshake before accepting any solve frame.
5. Send exactly one solve request.
6. Read bounded progress and result frames; capture bounded/redacted stderr separately.
7. Close stdin after sending the request.
8. Require exactly one terminal protocol frame and then process exit.
9. Kill the entire process tree on cancellation, parent timeout plus grace expiry, protocol violation, excessive frames/output, or failed cleanup.
10. Discard incomplete frames and delete the temporary directory on every terminal path.

A persistent pool is deferred until profiling demonstrates startup cost matters. It must not be introduced merely to reduce process count.

### Wire schema and evolution

Use a project-owned length-delimited Protocol Buffers stream on stdin/stdout. Stdout contains protocol frames only; safe text diagnostics go to stderr. Normalize the original package placeholder to the final project namespace:

```proto
syntax = "proto3";
package eutheto.worker.v1;

message HandshakeRequest {
  uint32 protocol_version = 1;
  string core_version = 2;
  string expected_backend = 3;
}

message HandshakeResponse {
  uint32 protocol_version = 1;
  string worker_version = 2;
  string ortools_version = 3;
  repeated string capabilities = 4;
}

message SolveRequest {
  string request_id = 1;
  bytes cp_model_proto = 2;
  bytes sat_parameters_proto = 3;
  repeated ProjectionRequest projections = 4;
  ResourceLimits limits = 5;
}

message WorkerEvent {
  string request_id = 1;
  oneof event {
    Started started = 2;
    Progress progress = 3;
    Incumbent incumbent = 4;
    Finished finished = 5;
    WorkerError error = 6;
  }
}
```

Do not reuse removed protobuf tags or change stable field semantics. Unknown-newer versions fail safely. Compatible minor additions rely on protobuf unknown-field behavior plus explicit capabilities; incompatible semantics increment protocol major and require an ADR. All size, nesting, count, and rate limits are centralized tested constants.

### Framing and initial caps

Each frame is exactly a 4-byte unsigned big-endian length followed by that many protobuf bytes. Reject truncated, malformed, or over-cap frames before allocation/deserialization. Initial centralized caps are:

| Input/output | Limit |
|---|---:|
| Handshake frame | 1 MiB |
| Solve request | 256 MiB |
| Individual worker event | 16 MiB |
| Total captured stderr | 4 MiB, then a truncation marker |
| Event count/rate | bounded and throttled in the parent |

These values are policy, not immutable ABI. Raising one requires benchmark and security review plus protocol metadata. Never deserialize unbounded nested input.

### Handshake and state machine

The parent sends the handshake first. Reject unsupported protocol major, unexpected backend ID, missing required capability, malformed version fields, and any solve frame received before handshake.

```text
START
  → HANDSHAKE_RECEIVED
  → HANDSHAKE_SENT
  → SOLVE_RECEIVED
  → STARTED_EVENT
  → zero or more PROGRESS/INCUMBENT events
  → exactly one FINISHED or ERROR event
  → EXIT
```

Any repeated solve, stale/mismatched request ID, event before `STARTED`, second terminal frame, or other out-of-order frame is a protocol violation.

### Exit codes and terminal interpretation

| Code | Meaning |
|---:|---|
| 0 | Terminal result/error frame sent successfully |
| 64 | Invalid invocation or protocol input |
| 65 | Invalid or unsupported model payload |
| 70 | Internal worker error |
| 71 | OR-Tools initialization or version error |
| 75 | Resource limit or temporary execution failure |
| 78 | Worker configuration or version mismatch |

The parent primarily trusts a consistent terminal frame. A missing or contradictory terminal frame plus exit code becomes `BackendFailed`; an exit code never converts an unverified partial candidate into success.

### Finished result and diagnostics

A finished frame contains raw CP-SAT status, normalized worker status, only requested projected variable values, objective values/bounds, wall/user/deterministic time when available, bounded conflicts/branches/propagations summaries, sufficient assumptions for infeasibility when available, bounded structured response stats, applied-parameters hash, and model fingerprint. The Rust supervisor separately records queue, worker startup/handshake, translation/serialization, first-incumbent, solver, protocol-decoding, and total-adapter spans so worker time is never presented as end-to-end product latency.

Allowed sanitized diagnostics are backend version, model counts, presolve reductions, incumbent score/bound, and terminal stats. The worker must never receive or print names, notes, AI content, credentials, or filesystem paths; the adapter uses numeric/stable IDs and retains human provenance in Rust. Arbitrary unbounded logs are neither captured nor persisted by default.

### Translation maps

The Rust adapter translates planning IR to `CpModelProto` and retains:

- planning variable ID → CP-SAT variable index;
- interval ID → interval-constraint index;
- planning constraint ID → all generated CP-SAT indices;
- objective and assumption maps;
- projection requests;
- upstream provenance.

Phase 03 supports Boolean variables, integer domains, clauses/conjunction/implication/equivalence, at-most-one, exactly-one, cardinality ranges, integer linear equalities/inequalities, enforcement literals, bounded objective terms, and the projection needed by its fixtures. Each primitive must either translate with exact semantics or be rejected during compatibility checking. Later primitives are added under the same contract rather than emulated incorrectly.

### Parameter allowlist

Never deserialize arbitrary user-provided `SatParameters`. Construct it from a project-owned allowlist:

- maximum wall time;
- worker count bounded by application/resource policy;
- random seed;
- stop after first feasible;
- intermediate-solution callbacks;
- advanced diagnostic progress logging;
- deterministic test profile.

Validate and reject out-of-range values; persist the final applied set/hash. Diagnostic assumption-core solving uses a single worker and non-optimization configuration where required by the pinned API.

Balanced initially tests approximately 2–3 seconds of CP-SAT time inside the provisional 3–5 second end-to-end interactive objective; Quick may stop after the first useful incumbent and Deep remains explicitly bounded. These are benchmark hypotheses, not proof or public latency guarantees. Phase 12 calibrates versioned defaults from whole-pipeline evidence, including startup, projection, independent verification, and rendering that Phase 03 cannot itself prove.

### Assumptions and infeasibility evidence

Diagnostic compilation may guard each explainable required rule with one assumption literal or a deliberate group. Foundational invariants such as variable-domain coherence are not user-relaxable assumptions. The worker returns CP-SAT's sufficient assumption set when available; it is not necessarily minimal. Phase 04 maps it through provenance and may run bounded deletion/re-solve shrinking.

OR-Tools issue `google/or-tools#5141` is a blocking 9.15 gate: presolve in 9.14/9.15 may return a literal not present in the submitted assumption set. Before enabling assumption-core evidence, pin and reproduce against the exact build, validate every returned literal against the adapter's assumption map, and either demonstrate an upstream fix/verified safe configuration or disable the capability and return an explicit unavailable diagnostic. Never mis-map an unknown literal or label such output a valid conflict. Assumption-core tests must cover presolve, literal polarity/index semantics, single-worker diagnostic configuration, and out-of-set rejection.

### Resource and cancellation controls

Apply best-effort platform controls:

- parent wall-clock deadline and explicit timeout grace;
- whole-process-tree termination;
- bounded stdout, stderr, event count, and event rate;
- private/restricted working directory and sanitized environment;
- no network permission or requirement;
- CP-SAT thread cap;
- memory limit where reliable platform APIs exist;
- lower priority for deep background solves where appropriate.

A memory cap is defense in depth. Planning-IR/model-size validation must reject unsafe models before launch. MVP cancellation hard-terminates the process tree, records `Cancelled`, discards incomplete frames, and cleans the temporary directory. A future graceful cancel message may be additive, but hard termination remains the safety fallback.

## Native build and packaging

### Pinned source derivation

`nix/ortools-worker.nix` owns the exact source build. It must fetch a fixed 9.15 tag/commit and hash only after all version gates pass; build supported native components with CMake/Ninja; compile the project worker; test protocol/trivial models in `checkPhase` when build binaries are executable; install only the worker, required dynamic libraries if any, licenses/notices, and solver manifest; and expose passthrough metadata for packaging checks.

The derivation/package name uses the final project namespace, for example `eutheto-ortools-worker`, not a source placeholder. Its metadata describes the pinned OR-Tools CP-SAT worker for `eutheto`, uses Apache-2.0 metadata, and exposes `ortools-worker` as its main program. Confirm 9.15 CMake flags from the pinned source; do not copy flags from another release. Required intent includes disabling examples, samples, upstream tests not needed for packaging, Python, Java, .NET, GLPK, proprietary solvers, and unrelated integrations. `protobuf`, `protoc`, generated upstream protos, and linked OR-Tools must remain matched.

The repository supports both `nix build .#ortools-worker` and a documented native Windows Visual Studio/CMake build of the same worker source/revision with an equivalent manifest. Static versus dynamic linkage is target-specific and must be chosen by measured package size, runtime reliability, dependent-library loading, and exact license payload.

### Solver manifest

The machine-readable manifest includes source URL, tag/commit, source archive hash, proto checksums, protocol version, worker version/hash, target and architecture, OR-Tools version, adapter version, capabilities, exact build flags, linkage/runtime libraries, license metadata/notices, and generated SBOM reference. The application validates expected manifest and executable location at startup and before use after path changes.

### Sidecar and one-install contract

The Tauri build copies/renames the worker using the target-triple external-binary convention. Capabilities grant execution only for this exact binary and expected argument pattern; strict custom-command capability enforcement must be registered rather than assuming `invoke_handler` alone restricts every window.

Packaging checks verify filename/executable bit, architecture, handshake, manifest hash, license payload, and absence of forbidden linked dependencies. MVP target artifacts are Windows x86_64 `.exe`, macOS arm64 and x86_64 binaries, and Linux x86_64 binary; Linux arm64 waits for reliable CI/end-user coverage and Windows arm64 is post-MVP. The desktop installer includes application, web assets, worker, manifest, licenses/notices, presets, migrations, and updater configuration. End users install no Rust, Node, Nix, Python, C++, Java, or OR-Tools runtime.

Record application, core API, scenario/domain/planning-IR, worker protocol, OR-Tools, and adapter versions together in the release manifest. Solver-affecting releases publish the solver manifest, exact-bundle SBOM/notices, checksums/signatures or attestations, provenance, source revision, build flags, lock/Nix inputs, artifact digests, and benchmark summary. Phase 03 supplies unsigned packaging-smoke inputs; Phase 11 owns protected signing/publication.

## Ordered work packages

1. **WORKER-001 — protocol, supervisor, and native worker:** define versioned messages, centralized caps, request-ID/state-machine validation, exit mapping, redacted stderr capture, launch abstraction, cleanup guards, cancellation, and fault taxonomy; implement the native worker handshake, one-request parser, bounded parameter application, model validation, callbacks, terminal result/error, exit codes, and safe diagnostics without domain data.
2. **SOLVER-001 completion — descriptors and compatibility:** implement OR-Tools capability descriptor, compatibility report, normalized status/progress records, deterministic router record, explicit fallback policy, remaining-budget propagation, and stable adapter timing/quality evidence.
3. **OR-Tools 9.15 gate:** run target build probes, primitive benchmarks, exact license/config inspection, protobuf compatibility checks, callback checks, and the assumption-core issue gate; record source/hash/proto/CMake/linkage decisions. Do not begin a distributable pin if a gate fails.
4. **WORKER-002 — pinned OR-Tools Nix/native build:** implement the explicit Nix derivation and equivalent Windows build from the same source pin; produce identical manifest semantics, run the applicable protocol/trivial-model checks, and inspect linked dependencies.
5. **WORKER-003 — Rust CP-SAT adapter, dependent on WORKER-002:** translate the initial Boolean/integer linear/cardinality subset; retain stable index, projection, objective, assumption, and provenance maps; reject unsupported semantics before launch.
6. **Process controls:** enforce deadlines, event/output caps, process-tree termination, private temporary directory, environment sanitation, thread limits, optional platform memory limits, cleanup, and crash recovery.
7. **Sidecar assembly:** target-triple rename/copy, exact-binary capability, manifest validation, architecture/license/forbidden-dependency checks, and packaged launch smoke inputs for all MVP targets.
8. **Evidence and handoff:** expose candidate assignments/evidence without acceptance; record model counts, worker startup, translation, first-incumbent, solver, terminal status/bound and bounded callback metrics; persist reproducibility metadata; and integrate Phase 04's quarantine/verification boundary.

## Tests and acceptance

### Protocol and worker contract tests

- handshake success; unsupported major, malformed fields, missing capability, unexpected backend, solve-before-handshake, and actionable version mismatch;
- all legal state transitions plus repeated request, out-of-order event, stale/mismatched request ID, duplicate/missing terminal frame, and contradictory exit/frame handling;
- boundary and over-bound tests for 1 MiB handshake, 256 MiB request, 16 MiB event, 4 MiB stderr, nesting/count/rate caps, malformed/truncated/oversized frames, and fuzzed frame parsing;
- trivial satisfiable, optimal, and infeasible models; invalid model status; every supported IR primitive and projection;
- raw-to-normalized statuses, time-limit with/without incumbent, proof-state accuracy, and no `Feasible`→`Optimal` mistranslation;
- deterministic fixed-seed/single-worker profile and persisted applied-parameter hash;
- callback throttling/coalescing and bounded progress/logging;
- remaining-parent-budget boundaries, deadline reached during startup/translation/solve, and proof that fallback cannot exceed the original end-to-end deadline;
- adapter timing evidence for startup/handshake, translation, first incumbent, solver and protocol decoding, with first-incumbent never mislabeled as first verified feasible;
- sufficient-assumption propagation, polarity/index mapping, non-minimal labeling, out-of-set rejection for issue #5141, and safe unavailable behavior;
- cancellation, timeout grace, process-tree cleanup, crash recovery, temporary-directory cleanup, stdout protocol purity, stderr truncation/redaction, and no sensitive domain text;
- worker version/manifest/hash/target change detection.

### Build and packaging tests

- Nix worker derivation builds from the exact fixed source and runs protocol/trivial-model checks where executable;
- native Windows build uses the same revision/protos and equivalent manifest fields;
- linked-dependency/license inspection proves no GLPK, GPL, proprietary solver, or unrelated language runtime is bundled;
- cross-platform path, target-triple naming, executable permissions, architecture, dynamic-library resolution, and Tauri capability scope;
- a packaged desktop smoke launches and handshakes with the bundled worker on Windows x86_64, macOS arm64/x86_64, and Linux x86_64 without system OR-Tools or language runtimes.
- deterministic primitive/small-model benchmark artifacts report raw solver and full worker-adapter timings, model counts, objective/bound and termination under fixed seed/thread/budget settings; Phase 03 does not claim end-to-end product targets from these microbenchmarks.

### Phase exit gate

Phase 03 exits only when feasible, optimal, and infeasible fixtures pass end to end through translation and worker return; mismatches are actionable; cancellation removes the process tree; one parent budget bounds startup, translation, worker execution, and fallback; required timing/quality evidence is emitted; malformed output/crash cannot crash or corrupt the application; all supported primitive semantics pass contract tests; no forbidden dependency is present; and every MVP target's packaged app can launch its worker. Phase 04 may still reject deliberate candidates, because Phase 03 never grants acceptance or measures first verified feasible itself.

Backend definition-of-done also requires approved distribution/license, visible exact version, capability matrix, cancellation/resource limits, normalized statuses, independent-verification integration, reproducibility metadata, contract tests, benchmark evidence, crash diagnostics, and packaged-target smoke coverage.

## Risks and failure handling

| Risk or failure | Required behavior |
|---|---|
| 9.15 fails a platform/build/benchmark/license gate | Block the pin and phase exit; record evidence. Do not silently select another version or weaken the target matrix. |
| Assumption core contains an unknown literal | Reject the evidence, mark the capability/result diagnostic invalid, retain sanitized details, and never render a mapped conflict. |
| Protobuf/protoc mismatch | Fail generation/build/handshake with explicit pinned-contract diagnostics. |
| Worker crash, native OOM, or missing terminal frame | Mark backend failed, quarantine partial data, kill descendants, clean temporary files, and offer a deliberate compatible fallback only under policy. |
| Invalid translated model | Surface compiler/adapter defect; no silent fallback and no user-infeasibility message. |
| Excessive frame/log/callback output | Terminate for protocol/resource violation, truncate with marker, and retain bounded diagnostics. |
| Stale result after scenario edit | Preserve recorded revision and prevent acceptance against the new revision; Phase 04 verifies only the matching revision. |
| Dynamic library absent or wrong architecture | Fail preflight/packaging check with target-specific actionable error; never search arbitrary system paths. |
| Untrusted names/notes reach native diagnostics | Treat as a privacy defect; adapter sends numeric IDs and Rust renders provenance. |
| Reproducibility differs due platform signing/timestamps | Scope claims accurately while recording source, toolchains, flags, and artifact digests. |

## Deferred and non-goals

- Persistent worker pools, graceful-only cancellation, concurrent portfolio solving, cross-solver decomposition, and user-installed external backend discovery.
- Automatic selection of experimental Pumpkin; that belongs to [Phase 08](08-pumpkin-backend-and-router.md).
- Full workforce/global-constraint vocabulary beyond the initial tested primitive subset; additions follow capability-first contract tests.
- Trusting CP-SAT's objective/status as the authoritative domain score or validity result.
- Python/Java/.NET wrappers, embedded Python, GLPK, proprietary solvers, GPL/AGPL solver dependencies, arbitrary `SatParameters`, network access, or `PATH` lookup in official builds.
- Public signing/notarization/updater publication; Phase 03 proves unsigned sidecar assembly and launchability.

## Assumption and version gates

Evidence date: **2026-08-29**.

- Candidate solver is OR-Tools **9.15**, the current stable release. Adopt it only after exact tag/commit, fixed source hash, proto checksums, all MVP platform builds, primitive benchmarks, callback behavior, CMake flags, linked-dependency/license inspection, static/dynamic linkage, runtime-library loading, manifest/SBOM, and assumption-core gates pass.
- OR-Tools 9.15 has a known P-critical-for-this-feature assumption-core concern (`google/or-tools#5141`): presolve may report a literal outside the submitted set. The capability remains gated as specified above; a sufficient core is never assumed safe merely because solving succeeded.
- Protobuf/protoc is pinned to **33.1**, matching the OR-Tools 9.15 source dependency. Generated Rust and C++ worker-protocol bindings and the linked C++ runtime must use that recorded toolchain; do not blindly update to protobuf 36.0.
- Rust remains **1.97.1** until a stable newer than 1.98.0 fixes the known P-critical vtable miscompilation; native worker tooling must be recorded from the committed Nix/toolchain pins.
- Exact archive hash, proto checksums, target linkage choice, and target runtime-library set are implementation outputs, not choices this roadmap fabricates.
- The project name `eutheto` is final. The working CLI name `optimizer`, reverse-domain application ID, portable file extension, hosting organization/governance contacts, and release-signing choices remain explicit repository/release gates.
