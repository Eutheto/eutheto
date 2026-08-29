# Phase 02 — Domain-pack and planning-IR contracts

[Previous: Phase 01](01-core-application-shell-and-persistence.md) · [Roadmap index](README.md) · Next: [Phase 03](03-ortools-worker-vertical-slice.md)

## Outcome

Official packs register behind a stable internal contract; a trivial pack can create/migrate/mutate/validate/compile/view/project/verify/explain without knowing a solver; immutable solver-neutral planning IR represents the complete MVP primitive vocabulary with provenance, lexicographic objectives, assumptions, projections, validation, capabilities, deterministic summaries/hashes, and connected components; solver adapters implement one backend contract and publish explicit compatibility. Invalid or unsupported models stop before backend execution.

## Source coverage

Blueprint §§12–14 and §29 Phase 2; relevant §§7, 9.6, 17, 20 foundation constraints, 24, 26, 31–33; Appendices A–D contract intersections, H, I (`PACK-001`, `IR-001`, `IR-002`, `SOLVER-001`, foundation of `VERIFY-001`), J.3, K.3–K.4/K.7, L; the cross-cutting [Performance and Solver UX Targets](performance-and-solver-ux-targets.md); and the pack-owned contracts in [Portable Data, Backup, and Result Sharing](portable-data-backup-and-sharing.md). This file defines contracts consumed by phases [03](03-ortools-worker-vertical-slice.md), [04](04-independent-verifier-and-explanations.md), [05](05-workforce-core-vertical-slice.md), [08](08-pumpkin-backend-and-router.md), [09](09-seating-domain-and-venue-experience.md), and [13](13-post-mvp-roadmap.md).

## Dependencies

Phase 01 supplies IDs, units, errors, document envelope/migrations, revisioned commands, application DTO generation, normalized persistence seams, cancellation/resource options, CLI/Tauri catalogs. Phase 02 must not depend on Tauri, SQLite, provider clients, OR-Tools, Pumpkin, or official pack implementations.

## Decisions and invariants

- Official MVP packs are compiled in and explicitly registered. Stable Rust traits separate crates; no unstable native plugin ABI or runtime dynamic libraries.
- Future third-party packs use a narrow sandboxed WASM/component host with memory/fuel limits, signed manifests, and no ambient filesystem/network; it is not implemented now.
- Domain model/Domain IR is pack-specific normalized human meaning. Planning IR is shared Boolean/integer/interval mathematics. Backend model is adapter-private. Never collapse these layers.
- Domain packs never invoke solvers, access SQLite/credentials/network, mutate outside command transaction, or render untrusted HTML.
- Backends never depend on official domain crates and return only assignments/evidence—not trusted domain solutions or authoritative scores.
- Planning IR is immutable, integer/Boolean-first, typed (no string DSL), serializable, deterministic, validated before routing, provenance-complete, and explicit about semantics/capabilities/projection.
- Required rules are mandatory constraints. Preferences are explicit bounded objective expressions, never secretly relaxable requirements.
- Every accepted solution must later have `feasibility == 0` under independent domain verification. Backend scalar objectives are not shown as authoritative results.
- Capability compatibility is exact and user-explainable before solve. An unsupported override is disabled/rejected rather than failing mid-solve.
- Router policy is deterministic code, never AI. One connected MVP model goes to one backend; splitting requires mathematical and domain-semantic proof.
- Canonical generation uses stable IDs/order and checked `i64` arithmetic. Compiler output depends only on revision plus explicit context/options—never wall clock, machine locale/core count, hash-map order, or ambient environment.
- `SolveOptions` carries one explicit end-to-end deadline/resource budget. Compilation, backend translation/execution, fallback, projection, verification, explanation, and nested diagnostics consume the remaining parent budget rather than resetting independent limits.
- A fast raw incumbent is not a product result. Only projection plus independent verification establishes `first_verified_feasible`; proof search never withholds an already verified incumbent.

## Domain-pack architecture

### Responsibilities

Each pack supplies, without abbreviating any responsibility:

- entities and required/preference rule types;
- domain-specific typed commands;
- structural and semantic validation;
- defaults, presets, and guided setup steps;
- deterministic normalization and compilation to planning IR;
- independent verification of normalized solutions;
- authoritative score interpretation;
- explanation vocabulary and evidence rendering;
- current-domain ↔ portable-domain conversion, portable schema/capability metadata, historical portable migrations, and domain-owned Share Result payload builders;
- purpose-built view-model builders;
- typed AI tool definitions and examples where appropriate;
- UI metadata and optional official Vue components;
- sample scenarios, boundary/infeasible fixtures, and benchmark cases.

It must not call OR-Tools/Pumpkin, access database or credentials, network during migrate/validate/compile/verify, mutate outside application transaction, or return pre-rendered untrusted HTML.

Pack portable conversion and migration operate on strict typed portable representations, are pure/offline/deterministic, and never inspect SQLite or archive paths. Pack Share Result builders accept only an accepted immutable result plus explicit privacy options and return typed inert data/view payloads; they do not render HTML, choose filesystem paths, or bypass provider redistribution policy.

### Official registry and descriptor

```rust
pub fn official_registry() -> DomainPackRegistry {
    DomainPackRegistry::builder()
        .register(WorkforceDomainPack::new())
        .register(SeatingDomainPack::new())
        .build()
        .expect("official pack registry must be valid")
}

pub struct DomainPackDescriptor {
    pub id: DomainPackId,
    pub display_name: LocalizedText,
    pub description: LocalizedText,
    pub pack_version: semver::Version,
    pub latest_schema_version: u32,
    pub icon_id: String,
    pub capabilities: DomainCapabilities,
    pub portable_schema_version: u32,
    pub portable_capabilities: BTreeSet<PortableCapabilityId>,
    pub share_result_schema_version: u32,
    pub documentation_url: Option<Url>,
    pub license: LicenseMetadata,
}
```

Startup `expect` is allowed only for this static developer invariant and must be clear. Registry rejects duplicate IDs, invalid versions/capabilities and incompatible descriptor/catalog combinations. CLI/desktop list descriptors without knowing pack Rust types.

### Core trait

```rust
#[async_trait]
pub trait DomainPack: Send + Sync {
    fn descriptor(&self) -> &DomainPackDescriptor;
    fn new_document(&self, preset: Option<PresetId>)
        -> Result<DomainDocument, DomainError>;
    fn migrate_document(&self, document: DomainDocument)
        -> Result<DomainDocument, MigrationError>;
    fn apply_command(
        &self,
        document: &DomainDocument,
        command: &DomainCommandEnvelope,
    ) -> Result<DomainMutation, DomainError>;
    fn validate_fast(&self, document: &DomainDocument) -> ValidationReport;
    fn validate_full(&self, document: &DomainDocument) -> ValidationReport;
    fn compile(
        &self,
        document: &DomainDocument,
        context: &CompileContext,
    ) -> Result<CompiledPlanningProblem, CompileError>;
    fn verify(
        &self,
        document: &DomainDocument,
        solution: &NormalizedSolution,
    ) -> VerificationReport;
    fn build_view(
        &self,
        document: &DomainDocument,
        solution: Option<&NormalizedSolution>,
        request: &DomainViewRequest,
    ) -> Result<DomainView, DomainError>;
    fn explain(
        &self,
        document: &DomainDocument,
        solution: Option<&NormalizedSolution>,
        evidence: &ExplanationEvidence,
        request: &ExplanationRequest,
    ) -> Result<DomainExplanation, ExplanationError>;
    fn command_catalog(&self) -> &DomainCommandCatalog;
    fn ai_tool_catalog(&self) -> &AiToolCatalog;
    fn ui_manifest(&self) -> &DomainUiManifest;
    fn export_portable(
        &self,
        document: &DomainDocument,
        context: &PortableExportContext,
    ) -> Result<PortableDomainDocument, PortableError>;
    fn migrate_portable(
        &self,
        document: HistoricalPortableDomainDocument,
    ) -> Result<PortableMigrationOutput, MigrationError>;
    fn import_portable(
        &self,
        document: &PortableDomainDocument,
        context: &PortableImportContext,
    ) -> Result<DomainDocument, PortableError>;
    fn build_share_result(
        &self,
        document: &DomainDocument,
        accepted: &AcceptedResult,
        options: &ShareResultOptions,
    ) -> Result<DomainShareResult, ShareResultError>;
}
```

Official packs stay strongly typed internally. The erased boundary contains pack ID, schema version, and JSON bytes/value; implementation deserializes once, validates typed data, and serializes mutation result. Slow work remains cancellable and never blocks async executor threads.

### Command catalog

Every command declares:

- stable namespaced ID such as `official.workforce.add_person`;
- strict JSON Schema for payload;
- result/change schema;
- localized title/description and human-summary formatter;
- permission/risk classification;
- reversibility;
- whether an AI proposal may group it;
- valid/invalid examples for docs and tool calling.

Generate checked-in TS types for every official command. Unknown future commands stay generic envelopes for clients that do not understand them. Catalog schema, Rust DTO, TS DTO and AI schema derive from one authoritative definition and drift checks.

### UI manifest

```rust
pub struct DomainUiManifest {
    pub setup_steps: Vec<SetupStepDescriptor>,
    pub entity_kinds: Vec<EntityKindDescriptor>,
    pub rule_kinds: Vec<RuleKindDescriptor>,
    pub result_views: Vec<ResultViewDescriptor>,
    pub importers: Vec<ImporterDescriptor>,
    pub exporters: Vec<ExporterDescriptor>,
}
```

It powers navigation, rule picker, consistent forms, AI documentation, and discovery. Purpose-built official components remain required for schedule grids/canvas. Do not attempt a fully schema-generated application.

### Pack compatibility

Open only if pack ID exists; current and portable schemas are supported or migratable; required capabilities are enabled; and every extension is understood or explicitly declared nonsemantic/ignorable. Unknown semantic capabilities block import before commit. When a pack is unavailable, show safe envelope metadata and allow byte-preserving re-export of the unopened original bundle only when that can preserve the exact archive safely; never reconstruct, down-convert, discard unknown data, or pretend validation succeeded.

## Planning IR contract

### Semantic levels and root shape

```text
pack document → normalized Domain IR → solver-neutral Planning IR
              → adapter-private model → candidate values
              → stable projection → normalized domain solution
```

```rust
pub struct PlanningProblem {
    pub schema_version: u32,
    pub variables: Vec<Variable>,
    pub constraints: Vec<ConstraintRecord>,
    pub objectives: ObjectivePlan,
    pub assumptions: Vec<Assumption>,
    pub projections: Vec<SolutionProjection>,
    pub provenance: ProvenanceIndex,
    pub metadata: PlanningMetadata,
}

pub enum Variable {
    Bool(BoolVariable),
    Int(IntVariable),
    Interval(IntervalVariable),
}

pub struct IntDomain {
    pub inclusive_ranges: Vec<(i64, i64)>,
}
```

Intervals explicitly carry start, duration, end, optional presence literal, and resource metadata. Adapter may use native interval or primitive translation. Continuous variables are out of MVP and may be added only in a new IR version without changing integer semantics.

### Exhaustive MVP primitive vocabulary

The IR builder, enum, serializer, validator, capability descriptor, diagnostic formatter, support matrix, and contract fixtures cover every unconditional primitive below and every conditionally enabled primitive:

1. Boolean clause / `BoolOr`;
2. Boolean conjunction / `BoolAnd`;
3. implication;
4. equivalence;
5. at-most-one;
6. exactly-one;
7. cardinality range;
8. integer linear equality and inequality;
9. reified linear comparison;
10. all-different;
11. allowed tuple table;
12. forbidden tuple table;
13. element/index lookup;
14. min helper;
15. max helper;
16. equality helper;
17. absolute difference;
18. no-overlap for intervals;
19. cumulative resource capacity;
20. optional intervals;
21. circuit/path is conditional: it remains absent unless a real first-domain need passes the gate by demonstrating necessity and specifying semantics and capabilities;
22. explicit objective penalty/reward expressions.

Every constraint has stable ID, optional enforcement literals, typed provenance, tags, and `required_capabilities()`:

```rust
pub struct ConstraintRecord {
    pub id: PlanningConstraintId,
    pub body: Constraint,
    pub enforcement: Vec<Literal>,
    pub provenance: ProvenanceRef,
    pub tags: BTreeSet<ConstraintTag>,
}
```

Define literal polarity, empty Boolean semantics, interval presence behavior, table arity/duplicates, linear coefficient/bound arithmetic, reification direction, optional global constraints and index out-of-domain behavior unambiguously in public API docs and fixtures.

### Required rules, preferences, and score

A preference compiles to violation/satisfaction expressions, bounded non-negative penalty or reward, objective level/category, and provenance for explanation. Domain document stores strength explicitly.

```rust
pub struct ScoreVector {
    pub feasibility: i64,
    pub levels: Vec<ScoreLevelValue>,
}
pub struct ScoreLevelValue {
    pub level_id: ScoreLevelId,
    pub value: i64,
    pub direction: OptimizationDirection,
    pub category_breakdown: BTreeMap<ScoreCategoryId, i64>,
}
```

Comparison is lexicographic with stable level identity/order and explicit min/max direction. Workforce recommended levels: required/hard locks; repair stability; high preferences; fairness/balance; ordinary preferences; deterministic ties. Seating: required placement/separation; manual locks; requested groups; undesirable proximity; balance/ordinary preferences; deterministic ties.

Adapters may safely scalarize only after finite bounds prove exact priority preservation and all multiplication/sums fit `i64`; otherwise use multipass lexicographic solve. Reject overflow-prone objectives. UI displays verified category breakdown, not scalar backend objective.

### Provenance

Every user-relevant variable, constraint, objective and projection maps:

```text
domain rule/fact ID → normalized rule + affected entity IDs
→ planning records → backend IDs/proto indexes → evidence → localized explanation
```

```rust
pub struct ProvenanceRecord {
    pub source_kind: ProvenanceSourceKind,
    pub source_id: String,
    pub entity_refs: Vec<DomainEntityRef>,
    pub message_key: String,
    pub parameters: serde_json::Value,
    pub parent: Option<ProvenanceRef>,
}
```

Never use display text as identity. Message key plus typed parameters supports localization and stable diagnostics.

### Normalized solution and projection

Define a backend-independent, versioned `NormalizedSolution` containing scenario revision, stable domain assignment IDs/entities, typed values/intervals, projection version, and optional backend evidence references—but not trusted feasibility/score. `SolutionProjection` maps validated planning variable expressions to stable normalized assignments, defines required/optional value behavior and unknown-variable rejection, and is deterministic under variable ordering. Projection failure is an adapter/compiler correctness error, not user infeasibility. Verification report and authoritative score are separate phase-04 outputs.

### Deterministic compilation pipeline

Every pack performs exactly:

1. parse and migrate pack document;
2. normalize defaults/generated instances;
3. structural validation;
4. semantic validation;
5. deterministic entity indexes;
6. precompute time, geometry, compatibility, conflict graphs;
7. create variables;
8. create required constraints with provenance;
9. create preference expressions/objective levels;
10. create projections;
11. validate planning IR;
12. compute capability requirements, component graph, size summary and canonical hash.

Compilation is pure for scenario revision plus `CompileContext`; “today,” horizon, locale, seed and limits are explicit.

Before Planning IR construction, packs deterministically remove candidates that are provably impossible under availability, eligibility, horizon, location/travel, resource, and other required-rule facts. They record pre/post counts and model-size estimates. Pruning must preserve the feasible set and authoritative score possibilities; exhaustive bounded fixtures compare pruned and unpruned semantics.

### IR validation

Reject before routing:

- duplicate IDs;
- missing variable/literal references;
- empty/non-normalized/overlapping integer domains;
- any `i64` coefficient/bound/objective arithmetic overflow;
- incoherent start + duration = end or optional interval equations;
- non-finite/unbounded objective bounds when required;
- non-Boolean assumption literals;
- invalid projection expressions;
- undeclared required capabilities;
- unsupported recursive/extension nodes;
- absent provenance for user-relevant records.

This is compiler defect/corrupt extension, never an infeasibility status.

### Canonical hash and summary

BLAKE3 covers IR schema version; normalized variables/constraints in stable order; objective plan; backend-relevant options; domain compiler version; adapter version. `PlanningProblemSummary` includes primitive/capability counts, variable/domain/constraint/objective sizes, coefficient ranges, optional/interval/global counts, component hypergraph summary, assumptions/projection counts and model hash—without private scenario text. A repeat solve/cache is reusable only after current verification against current revision.

## Backend API and routing contract

### Trait, descriptor, compatibility

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

pub struct CompatibilityReport {
    pub compatible: bool,
    pub unsupported_features: Vec<UnsupportedFeature>,
    pub warnings: Vec<CompatibilityWarning>,
    pub estimated_translation_cost: Option<ModelCostEstimate>,
}
```

Distribution is `BuiltIn`, `BundledWorker`, or `UserProvided`; stability `Stable`, `Beta`, `Experimental`. Report exact primitive/path/objective/option limitations and remediation. Backend result contains raw planning assignments, normalized termination/evidence and diagnostics references, never final domain status.

### Status and progress

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

Map backend “unknown” to the most accurate state based on verified incumbent and termination reason. Progress to Vue is throttled/coalesced; safe log lines are bounded/redacted. “Deep” never means optimal.

### Budget, timing, and quality evidence

The backend-neutral result and progress records preserve end-to-end deadline, remaining budget at dispatch, backend limit, seed/worker count, model-size summary, termination reason, first-incumbent time, first independently verified feasible time, backend objective/bound, and authoritative score reference. Phase spans use the stable taxonomy in [Performance and Solver UX Targets](performance-and-solver-ux-targets.md); backend-specific details remain bounded diagnostic extensions.

`percent` is populated only when a phase has a real measurable denominator. The application maps detailed events to truthful coarse user phases and applies the 300–500 ms display threshold in Phase 06. Callback counts, diagnostic lines, objective/bound updates, and progress events are bounded and coalesced before crossing process or Tauri boundaries.

Quick/Balanced/Deep are versioned policy profiles, not proof promises. The initial Balanced hypothesis places approximately 2–3 seconds of backend time inside a 3–5 second end-to-end interactive budget; Phase 12 calibrates final defaults from representative whole-pipeline benchmarks. `Deep` remains bounded and never means `Optimal`.

### Deterministic MVP router

1. Honor explicit override only when compatible.
2. Otherwise select OR-Tools CP-SAT for supported nontrivial official models.
3. Select native specialized algorithm only for compiler-recognized exact structure with independent verifier.
4. Never auto-select experimental Pumpkin unless experimental mode is enabled and generated matrix has no gap.
5. Split connected components only after hypergraph and domain proof.
6. Persist routing decision and all reasons.

Build hypergraph with variable nodes; every constraint or multi-variable objective connects referenced variables. Components are independent only if no constraint/objective term spans them, no global score normalization spans them, no projection/domain rule crosses them, and merged values cannot violate domain invariants. Fairness frequently connects assignments; time/location separation is not proof.

### Fallback policy

- unavailable/crash before candidate: configured compatible fallback may run;
- invalid model: no silent fallback; surface compiler/adapter defect and preserve diagnostics;
- proven infeasible: no performance-style fallback; explicit advanced diagnostic cross-check only;
- time limit without candidate: fallback only within remaining total budget;
- verification failure: quarantine, record critical defect, and only then optionally run a distinct backend with failure preserved.

Tests use fixed seed and deterministic/single-worker settings where available. Production may use threads and yield equivalent alternatives. Record seed, threads, backend/adapter/protocol version, model hash and all options.

Post-MVP portfolio execution shares one total budget, independently verifies every candidate, compares authoritative score vectors, safely cancels losers, preserves producer identity and avoids multiple heavy solvers on battery by default. It is not cross-solver decomposition.

## Generated support matrix

Create one machine-readable matrix and generate runtime/docs views with rows for every unconditional IR primitive and every primitive enabled after its gate, plus assumptions, multipass/scalar objectives, intermediate candidates, proof/bounds, cancellation, deterministic mode, resource limits, infeasibility evidence, and projection value types. Circuit/path has no row while absent; if its first-domain gate passes, its matrix row and tested fixture become mandatory. Columns cover each registered backend/version/adapter. Each cell is `supported`, `unsupported`, or `restricted` with restriction ID and tested fixture. Descriptor capability declarations must be derived/checked against this matrix. Phase 02 includes a deliberately unsupported fake backend; OR-Tools/Pumpkin cells remain unclaimed until phases 03/08.

## Rule implementation checklist

Every future official rule must complete all 13 items in the same change stream:

1. stable rule ID/schema and migration impact;
2. command DTO and validation;
3. plain-language editor;
4. fast/full domain validation;
5. planning-IR compilation;
6. backend capability/translation tests;
7. independent verifier evaluation;
8. provenance and deterministic explanation;
9. AI tool/schema when appropriate;
10. working-CLI/document-format example;
11. edge and infeasible fixtures, including empty/default scope and relevant DST/boundary cases;
12. user docs and limitations;
13. benchmark/model-size impact.

A rule is never complete when only formulation works. Before 1.0, schema/bundle migrations and no-data-loss behavior remain required even while internal APIs/CLI flags may evolve with release notes.

## Ordered work packages

1. **PACK-001 — Registry/descriptors.** Duplicate/incompatibility checks, official compiled-in registration seam, pack availability behavior, descriptor/catalog/UI manifest schemas, portable/share schema versions and capability declarations.
2. **PACK-002 — Erased/typed contract.** Domain document; internal and portable migration/conversion; new/apply/fast/full/compile/verify/view/explain; accepted-result→Share Result contribution; cancellation and pure-context boundaries; test pack.
3. **PACK-003 — Generated catalogs.** Command/result/portable/share JSON Schema, TS DTOs, AI metadata, examples, risk/reversibility, setup/entity/rule/view/import/export descriptors.
4. **IR-001 — Values/builders.** Typed IDs/literals/expressions/domains/Boolean/integer/interval variables and an exhaustive enum with shape-safe builders for every unconditional primitive and any primitive enabled after its gate.
5. **IR-002 — Objectives/provenance/projection.** Score levels/categories/directions/bounds, assumptions, provenance index, normalized solution/projection, stable diagnostic serialization.
6. **IR-003 — Validation/summary/hash.** All validation failures, checked arithmetic, capability extraction, size/cost summary, canonical BLAKE3 and deterministic ordering.
7. **SOLVER-001 — Backend contract.** Descriptor/distribution/license/stability, compatibility report, result/evidence, normalized status, truthful progress, cancellation/resource semantics, first-incumbent/termination timing, bounded diagnostics and fake backends.
8. **SOLVER-002 — Router/components/fallback.** Deterministic policy, override, hypergraph plus domain merge proof, decision record, versioned Quick/Balanced/Deep policy and one total-budget fallback.
9. **SOLVER-003 — Matrix/generation.** Machine source, generated docs/runtime table, descriptor consistency and fixtures for every unconditional or enabled primitive.
10. **Vertical contract fixture.** Trivial pack with Boolean/integer/optional interval, required constraint, bounded preference, projection/provenance, compatible fake exact backend and independent fake verifier path; expose through working CLI/Tauri metadata.
11. **School-timetabling foundation fixtures.** Encode one bounded miniature timetable through both the pre-enumerated pattern-choice and occurrence-variable formulations, including explicit meeting-to-room links, projection/provenance and valid/infeasible variants; this proves IR expressiveness without implementing the school pack or UI.

## Tests and acceptance

### Required layers

- Pure unit tests for literal/domain normalization, every builder invalid shape, checked coefficient/bounds, score comparison/bounds, provenance graph, projection and compatibility.
- Property tests: normalized domain represents exactly values; serialization/hash invariant to insertion/entity ordering; score comparison transitive; builder output validates; proven-independent split/merge equals whole semantics; changing compiler/adapter/options changes hash as specified.
- Golden tests serialize stable semantic IDs/normalized expressions, not debug formatting; reviewer confirms meaning of changes.
- Pack contract tests for descriptor/catalog consistency; current/internal and current/historical portable schema validation/migration; semantic capability and extension handling; portable round-trip; Share Result data minimization/accepted-result gate; command/inverse seam; fast/full reports; compile purity; unavailable-pack exact preservation.
- Backend contract fixtures run against every claimed matrix cell; unsupported feature must fail compatibility without invoking solve.
- Metamorphic fixtures: rename display metadata leaves hash/meaning; add irrelevant inactive entity does not change feasible set; tightening minimum cannot expand set; preference→required cannot create assignments.
- Fuzz/property targets for IR parser/validator, domains, expressions, projection, component graph and malicious extension nodes.
- Budget/timing contract tests prove nested fallback and diagnostics receive only remaining time, raw incumbents never become accepted results, callback/progress volume is bounded, and every normalized termination has exact feasible/proof wording.
- Architecture tests enforce no domain→backend/Tauri/SQLite/provider and no backend→official-domain dependencies.

### Acceptance fixtures

1. **Trivial feasible pack:** one Boolean and one bounded integer, one required linear relation, one preference, projection and complete provenance; deterministic compile twice gives identical diagnostic/hash.
2. **Optional interval pack:** coherent start/duration/end/presence, no-overlap/cumulative capability extraction; absent interval projects according to explicit semantics.
3. **Every applicable primitive corpus:** one valid and invalid fixture for every unconditional primitive, plus circuit/path only if its first-domain gate has passed and it is enabled; enforcement literal, assumptions, projection and objective bounds.
4. **School pattern-choice foundation:** pre-enumerate finite valid meeting-pattern/room-combination options for a miniature section and select exactly one. The selected option activates every required meeting occurrence and its linked room assignment; an unselected meeting cannot claim a room and a room assignment cannot float without its meeting. Valid and infeasible variants exercise teacher/cohort/room no-overlap, room capacity/equipment eligibility, required meeting count and linked lecture/lab ordering or separation.
5. **School occurrence-variable foundation:** create assignments for each required occurrence × allowed period × eligible room in the same miniature scenario. Exactly one period-room pair is selected per occurrence; meeting placement and room presence are linked in both directions; required count/pattern and linked-component constraints hold; teacher/cohort/room no-overlap rejects collisions. Valid, missing-occurrence, double-room and unlinked-room variants are mandatory.
6. **Cross-formulation school gate:** exhaustive enumeration of the bounded scenario projects the same feasible meeting/room schedules and score under both formulations. Capability extraction, provenance and projection name stable section, occurrence, meeting-pattern and room identities; the gate fails if either formulation permits a floating meeting/room link. This is a foundation contract fixture, not a complete school domain implementation.
7. **Invalid IR:** duplicate/missing IDs, empty/non-normalized domains, overflow, incoherent interval, invalid assumption/projection, undeclared capability, unsupported recursion, missing provenance—all rejected before router; backend invocation counter stays zero.
8. **Unsupported fake backend:** exact unsupported list/warnings shown in working CLI/Tauri backend metadata; override rejected without solve.
9. **Connected components:** truly independent model splits/merges identically; a shared fairness objective, projection relationship and domain merge invariant each prevent split.
10. **Fallback:** unavailable permits budgeted fallback; invalid model/infeasible do not; verification failure is quarantined and recorded; timeout respects one total budget.
11. **Pack unavailable/newer:** metadata visible; original bundle remains unmodified and exactly re-exportable only under the safe byte-preservation path; unknown semantic capability/newer schema cannot import or be reconstructed; no validation/compile claim.
12. **Portable/share contracts:** test pack current export/import is semantically equivalent; historical migration is deterministic; current-only export has no down-conversion; unknown semantic extension fails while declared nonsemantic extension preserves; Share Result excludes source-only fields and rejects an unaccepted result.
13. **Architecture graph:** automated dependency test proves no forbidden crate edges.

### Exact exit criteria

- trivial test pack follows parse/migrate/normalize/validate/compile/IR-validate/summary/hash/project path;
- all invalid IR fixtures fail before routing with typed compiler errors;
- every unconditional primitive has stable semantics, builders, capabilities, diagnostic form and fixtures; circuit/path remains absent and is not a phase-exit dependency unless its first-domain gate passes, after which it must meet the same contract;
- both school foundation formulations pass their valid/infeasible/link-integrity fixtures, produce equivalent projected meeting/room schedules on the bounded corpus, and require no backend- or school-UI-specific IR semantics;
- command catalogs generate TS and AI metadata from one source;
- no domain crate depends on any backend/Tauri/SQLite/provider crate and no solver depends on official domains;
- golden and property suites pass deterministically under fixed context;
- support matrix is machine-readable, generated, runtime-visible and descriptor-consistent;
- compatibility/router/component/fallback fixtures prove no known unsupported solve starts;
- normalized solution/projection and provenance retain stable domain identities without trusting backend claims;
- score scalarization rejects any priority/overflow uncertainty.
- one-parent-budget, termination, first-incumbent/first-verified-feasible timing, and bounded-progress fixtures agree across fake backends and expose no fabricated completion percentage;
- pack portable schema/capability declarations, sequential migration hooks, current export/import, semantic-extension rejection, nonsemantic preservation, and accepted-only Share Result payload pass the synthetic conformance kit;

## Risks and failure handling

| Risk | Required response |
|---|---|
| Prematurely generic plugin system | Official compiled packs and internal trait only; sandbox SDK deferred. |
| IR mirrors CP-SAT | Semantic typed primitives, independent fixtures, and adapter-free crate graph. |
| Constraint semantics ambiguous | Public per-node semantics plus valid/invalid/boundary fixtures before adoption. |
| Objective overflow/priority loss | Finite bound proof or multipass; reject unsafe scalarization. |
| Missing provenance | IR validation failure, not best-effort explanation. |
| False decomposition | Hypergraph plus projection/domain merge proof; fairness tests; no split on appearance. |
| Capability drift | Generated matrix is executable contract; descriptor consistency check. |
| Backend result treated as truth | Result is candidate values only; projection then phase-04 verification. |
| Non-deterministic hashes/goldens | BTree/stable sort, fixed context and insertion-order properties. |
| OR-Tools assumption bug leaks into contract | Model sufficient—not minimal—evidence; gate version/issue #5141 in phase 03; never assert returned literals are valid without adapter validation. |
| Pumpkin pre-1.0 assumptions | Matrix remains unclaimed until phase 08 tests v0.5.0 actual API/cancellation. |

Stop and write an ADR if a domain rule cannot be independently verified, backend needs domain knowledge, decomposition lacks proof, new continuous/recursive/custom DSL semantics are needed, or correctness relies on undocumented backend behavior.

## Deferred and non-goals

- No real OR-Tools translation/worker (phase 03), authoritative verifier/explanation engine (phase 04), official workforce/seating implementation (phases 05/09), Pumpkin (phase 08), AI providers (phase 10), continuous LP/QP, portfolio execution, or cross-solver decomposition.
- Circuit/path remains absent unless a real first-domain need passes its gate by demonstrating necessity and specifying semantics/capabilities; it is not a Phase-02 exit dependency before then and is never silently approximated.
- No dynamic pack loading, marketplace, schema-generated whole UI, arbitrary DSL, or cached-result trust.
- The trivial fake backend exists only in tests and cannot appear as a production solver.

## Assumption and version gates

- IR schema version, stable field IDs/names and external camelCase are fixed before first public fixture; removed protobuf tags/serialized fields are never reused.
- Define numeric limits for variable/constraint counts, ranges, coefficients, objective levels/terms, provenance depth/parameters, projections and component graph before untrusted diagnostic import; tests enforce each.
- OR-Tools 9.15 stays a candidate until phase 03 verifies exact source/hash/protos/CMake/linkage/targets/callbacks and issue #5141. Assumption core is sufficient, not minimal; diagnostic core minimization requires documented single-thread/non-optimization conditions.
- Pumpkin 0.5.0 stays experimental candidate until phase 08 generates support from actual APIs; its solver is `!Send + !Sync`, must live on a dedicated thread, and cancellation is cooperative polling that may lag during long propagation.
- Protobuf/prost selection matches the pinned worker protocol; never take protoc 36.0 solely because it is newest.
- School foundation checks prove both pre-enumerated pattern-choice and occurrence-variable formulations, including bidirectional meeting/room links, teacher/room/cohort no-overlap, room capacity/equipment, required occurrences and linked-component ordering/separation, without implementing the school pack or UI.
- Domain fairness/default/weights, DST/rolling-hours/repair semantics and seating geometry meanings remain official-pack evidence gates, not generic IR guesses.

## Exit gate

Phase 02 exits only when every exact criterion and fixture passes, generated contracts/matrix have no drift, and crate-graph proof preserves dependency direction. Then [Phase 03](03-ortools-worker-vertical-slice.md) may implement the first real backend without changing domain semantics.
