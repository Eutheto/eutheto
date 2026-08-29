# Phase 13 — Post-MVP Roadmap

## Outcome

Extend the released local-first `eutheto` platform without weakening public-MVP contracts. The immediate release delivers the complete official school-timetabling pack, deeper explainability, stronger verified portfolio routing and alternative generation, and a separate Nuxt documentation/community website. Later branches may add sandboxed domain packs, optional solver adapters, specialized algorithms, new targets, richer imports, collaboration/server mode, enterprise integrations, additional official domains, improved AI, optional telemetry, and optional hosted services. Every branch remains independently gated, capability-scoped, license-reviewed, resource-bounded, migration-safe, accessible, and optional relative to the open-source core and desktop application.

The public-MVP baseline is defined in [Phase 11](11-public-mvp-packaging-and-documentation.md) and approved in [Phase 12](12-stabilization-and-public-release-gate.md). Version and decision evidence is in [the assumptions ledger](assumptions.md).

## Source coverage

This phase fully maps blueprint Section 20; immediate/later scope in Sections 6.4–6.5; Section 12.2's post-MVP pack model; Section 14.11; Sections 16.3–16.6; Sections 24.9 and 24.12; future targets and distribution options in Section 27; every branch in Section 30; Appendix C.8; and applicable validation/definition-of-done/stop-condition gates.

## Dependencies

- The Phase-12 public MVP provides the authoritative headless Rust core, versioned scenario/domain/Planning-IR contracts, reversible commands, transactional persistence/migrations, independent verifier, workforce and seating packs, bundled OR-Tools worker, experimental Pumpkin gate, desktop, CLI, exports, security/privacy controls, and release pipeline.
- Changes remain backward-compatible within semantic-version policy or intentionally trigger a versioned migration and compatibility notice.
- Every new pack, rule, backend, service, provider capability, target, and serialized field completes the same definitions of done as public-MVP features.
- Desktop and CLI remain first-class without accounts, servers, hosted services, telemetry, proprietary solvers, third-party packs, or AI.

## Decisions and invariants

- Project name is `eutheto`; working CLI name `optimizer` remains unresolved until explicitly finalized.
- School timetabling is compiled and explicitly registered for its first release; a plug-in marketplace/native ABI is not a prerequisite.
- Backend candidates never become authoritative without independent domain verification and authoritative score comparison.
- A pack that compiles rules also supplies independent verification semantics. Backends stay domain-agnostic; packs do not depend on solvers; verifiers do not trust backend status.
- No untrusted native dynamic libraries load in-process. Third-party packs use a sandboxed component model with no ambient authority.
- Process boundaries do not waive licensing review. Blocked/proprietary components remain outside official bundles absent an intentional reviewed policy change.
- Portfolio execution shares one global budget, verifies each incumbent, compares authoritative score vectors, cancels safely, preserves provenance, and respects battery/resource constraints.
- Telemetry, collaboration, servers, remote solves, AI gateways, and hosted services are optional. Opening, validating, solving locally, verifying, and exporting never require them.
- Legal/regulatory presets remain non-authoritative starting templates unless separately maintained and jurisdiction-reviewed.

### At/after-1.0 compatibility contract

Every branch delivered at or after 1.0 must:

- publish and honor a documented support window for scenario and bundle formats;
- preserve the public CLI and pack API according to semantic versioning;
- announce deprecations and retain deprecated public behavior for the documented deprecation period before removal;
- document worker-protocol compatibility ranges for each release or bundle workers that match the release's protocol exactly.

## Delivery branches and ordering

### Immediate post-MVP

1. Complete official school-timetabling pack.
2. Deeper explainability and explanation-quality evaluation.
3. Stronger local portfolio routing and explicit alternative/diversity generation.
4. Nuxt documentation/community site separate from the desktop Vue/Vite SPA.

### Later post-MVP

- sandboxed domain-pack SDK/catalog;
- native specialized algorithms;
- optional HiGHS, SCIP, MiniZinc, commercial-solver, and routing adapters;
- collaboration/server and enterprise integrations;
- additional official domain packs;
- richer imports and structured import assistance;
- AI/local-provider improvements;
- optional anonymous diagnostics;
- Linux arm64, Windows arm64, and additional package/repository targets;
- optional hosted services that never gate the local product.

## Branch A — Complete official school-timetabling pack

### Domain contract

```rust
pub struct SchoolScenario {
    pub academic_calendar: AcademicCalendar,
    pub cycle: ScheduleCycle,
    pub periods: Vec<Period>,
    pub teachers: Vec<Teacher>,
    pub students_or_cohorts: Vec<Cohort>,
    pub rooms: Vec<Room>,
    pub courses: Vec<Course>,
    pub sections: Vec<CourseSection>,
    pub meeting_patterns: Vec<MeetingPattern>,
    pub requirements: Vec<TimetableRule>,
    pub preferences: Vec<TimetablePreference>,
    pub locks: Vec<MeetingLock>,
}
```

Every field is versioned, validated, migratable, import/export round-trippable, and typed. Prefer cohort/conflict-group modeling unless an accepted use case requires individual student data.

### Supported calendars and patterns

The first release supports every-class/every-day period schedules, college-like/block patterns with longer classes on selected days, rotating A/B or custom cycle days, and one weekly all-class/special-activity day.

A cycle is an ordered list such as `Mon, Tue, Wed, Thu, Fri` or `A1, B1, A2, B2, Flex`. The academic calendar maps dates to cycle days with holidays/exceptions. Solve over the representative cycle, then deterministically project to dates with tested time-zone/DST, exception, and export behavior.

Meeting patterns are data rather than UI branches and include:

- daily one-period meeting;
- Monday/Wednesday/Friday;
- Tuesday/Thursday double period;
- four meetings plus one lab block;
- one long weekly seminar;
- custom allowed pattern set.

A section references teacher options, cohorts, required meetings, allowed patterns, room requirements, duration, capacity, and linked lecture/lab components. Candidate generation rejects impossible teacher/cohort/room/time/capacity/equipment/pattern combinations before Planning-IR variable creation.

### Required-rule catalog

Each rule has unambiguous semantics, validation, compiler mapping, independent verifier, explanation, import/export, migration, boundary/infeasible fixtures, and model-size evidence:

1. teacher meetings do not overlap;
2. room meetings do not overlap;
3. student/cohort meetings do not overlap;
4. room capacity covers enrollment;
5. room equipment/type satisfies the section;
6. each section receives a valid meeting pattern;
7. all required meeting occurrences are placed;
8. teacher availability;
9. room availability;
10. cohort availability;
11. linked lecture/lab ordering or separation;
12. required spacing across cycle days;
13. maximum consecutive teaching periods;
14. lunch/flex protections;
15. fixed meetings and hard locks;
16. shared-resource capacity.

### Preference catalog

Each preference has explicit scope, metric, direction, weight/preset, score contribution, explanation, and deterministic ties:

1. teacher preferred periods/days;
2. avoid first/last periods;
3. spread meetings across the cycle;
4. keep a section in one room;
5. minimize teacher/learner room changes;
6. prefer room proximity;
7. avoid long gaps in teacher/student schedules;
8. balance difficult courses across days;
9. preserve an existing timetable;
10. align shared planning periods;
11. prefer specific pattern families.

Users distinguish requirements from preferences and inspect raw distributions/targets rather than an unexplained quality score.

### Modeling

Benchmark two formulations per section/domain shape:

1. **Pattern choice:** pre-enumerate valid meeting-pattern/room combinations and select exactly one.
2. **Occurrence:** create assignment variables for each required occurrence, candidate period, and candidate room with linking constraints.

Pattern choice fits finite meaningful patterns; occurrence variables fit custom calendars. Planning IR supports both. Precompute teacher/cohort/room conflict graphs and never create impossible candidates. Formulation selection is deterministic, benchmark-backed, and visible in model summaries. Preserve entity provenance for verification/explanations.

### Setup and results UX

1. Define cycle days and periods from presets.
2. Add/import teachers, rooms, cohorts, courses, and sections.
3. Map enrollment/conflict groups.
4. Define reusable meeting patterns.
5. Set teacher/room availability.
6. Add required rules and preferences.
7. Validate data and inspect the conflict graph.
8. Optimize.
9. Review by section, teacher, room, cohort, and cycle day.
10. Lock edits and repair.
11. Project the cycle to the academic calendar and export.

Use purpose-built pattern editors and timetable grids, not a giant generic rule form. Imports provide templates, bounded streaming parsing, field mapping, preview, rejected-row reports, and transactional apply. Results provide status/score/evidence, filters, explanations, lock/repair, alternatives, calendar projection, and accessible printable/CSV/calendar exports. All paths support keyboard, screen reader, zoom/high-DPI, non-color cues, and measured virtualization.

### Acceptance scenarios

Mandatory fixtures:

- traditional five-day high-school schedule;
- A/B block schedule;
- college-style M/W/F and T/Th patterns;
- lab plus lecture coupling;
- shared teachers across departments;
- room equipment constraints;
- infeasible cohort conflicts;
- repair after teacher unavailability;
- large generated benchmark.

Each records hash/seed, schema version, size metrics, expected status when known, required-rule/score invariants, permitted backends, time budget, and baselines. Add exhaustive small truth/differential/mutation tests, property/metamorphic tests, every rule's valid/invalid/edge/empty fixtures, compiler goldens, migrations, import/export round trips, keyboard/accessibility, packaged E2E, explanation wording, performance, and usability review with school practitioners.

### School exit gate

Every entity, required rule, preference, pattern, formulation, view, import, projection, lock/repair, explanation, and export above passes its definition of done; compiler/verifier agree against exhaustive truth sets; all nine acceptance scenarios pass; practitioners review defaults and semantics; large models meet budgets; and committed fixtures contain no identifying real-world data.

## Branch B — Solver portfolio and alternatives

Deliver CP-SAT parameter-profile portfolios, concurrent OR-Tools/Pumpkin for exactly compatible subsets, verified-incumbent sharing where safe, local resource-aware scheduling, benchmark-trained deterministic routing, and explicit diversity objectives.

- Allocate one total wall-clock/CPU/thread/memory/log budget across children.
- Project, score, and independently verify each candidate.
- Compare authoritative lexicographic score vectors, never backend objective strings.
- Preserve backend/version/profile/seed/budget provenance.
- Cancel losers safely and account for cooperative cancellation delay.
- Avoid multiple heavy solvers by default on battery-constrained systems.
- Alternatives declare diversity metrics and remain required-rule feasible.
- Opaque remote AI never selects safety-critical backend behavior.
- Cross-solver decomposition of one connected component is separate research requiring proof of independence and merge equivalence.

Pumpkin 0.5.0 needs actual support matrix, dedicated-thread ownership, cooperative cancellation/time-limit evidence, verifier/contracts, and benchmarks before automatic routing. OR-Tools retains all K.3 gates.

## Branch C — Deeper explainability

Deliver more efficient bounded minimal-conflict shrinking; multiple independent conflict sets; causal repair explanations; visual rule-dependency graphs with non-visual equivalents; sensitivity analysis; preference shadow-price-like summaries only where mathematically meaningful; richer counterfactual batches; and explanation-quality evaluation with domain users. Preserve sufficient-versus-minimal, causal-versus-correlated, and configured-rule-versus-legal-certification distinctions. Add cancellation/resource budgets, provenance, replay, and certainty-wording tests.

## Branch D — Native specialized algorithms

Add project-owned Rust or permissively licensed crates only when the router proves exact structure:

- bipartite matching;
- minimum-cost assignment;
- connected components;
- shortest path;
- minimum-cost flow;
- graph-coloring heuristics used only to seed a full solver;
- geometry preprocessing and spatial indexing.

Results still flow through normalized projection, authoritative scoring, and independent verification. Require capability proof, property/differential tests, cancellation/resource behavior, benchmark benefit, license review, and an ADR where routing changes materially.

## Branch E — Additional solver adapters

### HiGHS

Reverify current HiGHS (discovery value 1.15.1) at implementation. Use for LP/MIP/QP subsets. Prefer an out-of-process worker unless a stable Rust binding, cancellation, isolation, packaging, and licensing case is proven. Inspect HiPO/dependency licenses.

### SCIP

Reverify current SCIP (discovery value 10.0.3) and use only where benchmarks show benefit. Versions before 8.0.3 are ineligible under their former ZIB Academic License. Inspect SoPlex/GCG/IPOPT and all assembled components.

### MiniZinc

Use for research, comparison, and prototyping. Scenario/Planning IR remain authoritative. An adapter may export compatible subsets to MiniZinc/FlatZinc and invoke installed or separately reviewed permissive solvers. MiniZinc 2.10.0 is MPL-2.0, but bundled solvers/components have distinct licenses; review each exact component.

### Proprietary solvers

Gurobi, CPLEX, Xpress, and peers are user-provided only: never ship binaries/license material; discover only after explicit action; display provider requirements; isolate optional adapters; never require them to open, verify, migrate, or export.

### Routing worker

A vehicle/service-routing worker is permitted only behind domain-neutral capabilities, verifier, license, process, packaging, and benchmark gates.

Every backend needs exact version/hash/license manifest, capability matrix, translation tests for every claimed IR node, status/cancellation/timeout normalization, malformed-output/crash isolation, benchmark corpus, independent verification, target packaging/update strategy, accurate stability label, and an ADR establishing material benefit.

## Branch F — Sandboxed domain-pack SDK and catalog

Official school remains compiled. Third-party packs use a versioned WASM component ABI with a narrow typed host API, deterministic memory/fuel/time/output limits, signed manifests, and no ambient filesystem/network/process/credential access. Native `.dll`, `.dylib`, or `.so` loading is forbidden.

Manifest fields:

- pack ID/version/license;
- required host API version;
- requested capabilities;
- memory/fuel limits;
- signed package digest;
- UI contribution type;
- migration functions.

SDK supplies schema, command, validation, compile, verify, explanation/view, import/export, migration host interfaces; constrained UI extension slots; local pack manager; compatibility/downgrade/recovery; signatures; optional Nuxt catalog; and a conformance kit. A compiler-capable pack must supply verification semantics. Official review complements, never replaces, sandboxing.

## Branch G — Nuxt documentation/community website

Use a separate Nuxt site for landing pages, searchable docs, domain/solver capability catalogs, rule reference, tutorials/examples, contributor docs, downloadable releases/checksums/signatures/SBOM/provenance, optional pack catalog, and benchmark dashboards. Keep desktop Vue/Vite SPA separate. Site remains accessible and privacy-minimizing; no account/telemetry is required to read docs or download releases.

## Branch H — Collaboration and service mode

The provisional `optimizer serve` uses a separately designed authenticated API, never exposed Tauri commands. Initial local mode binds loopback, uses explicit tokens, and warns that internet exposure requires a hardened profile.

Potential scope: local network service, authenticated organization server, scheduled jobs, webhooks/import pipelines, review/approval, shared history, and role-based permissions. Before implementation design concurrency/revisions, audit, authentication/session/token lifecycle, authorization/RBAC, tenant isolation, quotas, cancellation, secrets, backup/restore, TLS/proxy deployment, abuse prevention, migration/rollback, incident response, retention/privacy. Desktop remains server-independent.

## Branch I — Enterprise integrations

Optional directory/HRIS import, calendar sync, school information systems, event/guest platforms, signed policy packs, user-provided solver discovery, server SSO, and audit/report exports. Keep vendor fields out of core models. Each adapter needs typed boundary, disclosed data scope, credentials, rate/retry/cancellation, conformance fixtures, version mapping, preview/transactional apply, audit/redaction, license review, offline/failure behavior, and removal/migration path.

## Branch J — Additional official packs

Candidates:

- volunteer scheduling;
- conference/session scheduling;
- sports tournament scheduling;
- exam timetabling;
- room/resource booking;
- maintenance scheduling;
- manufacturing/job-shop planning;
- task/team assignment;
- vehicle/service routing;
- housing/roommate assignment;
- election-worker scheduling with jurisdictional caution.

Select on reusable primitives, domain maintainers, and clear UX—not modelability alone. Define privacy, rules/preferences, imports/exports, IR needs, verifier, explanations, accessibility, benchmark corpus, practitioner review, migration, ownership, and disclaimers before commitment.

## Branch K — AI and import improvements

Potential additions: domain-guided conversations, local-model capability profiles, structured import assistance, reusable typed rule macros, provider OAuth only through official flows, organization provider policies in server mode, deterministic redaction/pseudonymization, and optional voice as another typed-tool client.

Every write remains typed, validated, previewed, explicitly applied, revision-checked, and undoable. AI never generates backend code, bypasses validation/verification, invents facts, accepts its own writes, reads arbitrary files, executes shell, or receives credentials. Reverify current provider contracts and local capability warnings. Import assistance uses bounded parsers and transactional preview/apply.

## Branch L — Optional anonymous diagnostics

If added after the no-telemetry MVP: explicit opt-in; publish schema/collection code; collect no scenario content or person/guest/student names; permit inspection/deletion; keep full functionality when disabled. Before release define purpose, event/field catalog, pre-send preview, retention, endpoint ownership, transport, sampling, deletion, versioning, privacy obligations, and re-identification assessment.

## Branch M — Future targets/distribution

Potential additions after reliable CI/end-user testing:

- Linux arm64 desktop/CLI/worker;
- Windows arm64 when Tauri, WebView2, OR-Tools, signing, runners, and demand justify it;
- rpm or adjusted AppImage/deb mix;
- universal macOS bundles;
- signed apt/rpm repositories;
- additional updater/catalog hosting.

Each completes the same one-install, worker, license/SBOM/provenance, signing, updater, migration, offline, accessibility, E2E, and clean-machine gates as Phase 11/12. Development-shell availability is not release support.

## Branch N — Hosted services, if ever added

Potential update/catalog hosting, shared projects, remote solves, and managed AI gateways remain optional and never gate open-source core/desktop. Separate open-source capabilities from paid services. Require independent governance, tenancy, security/privacy/compliance, auth, retention/deletion, audit, availability/recovery, abuse controls, compatibility, export/exit, incident response, and disclosure.

## Ordered work packages

### Immediate wave

1. School schema/calendar/cycle/pattern/section contracts and migrations.
2. All 16 school required rules and 11 preferences: validation/compiler/verifier/explanations.
3. Conflict graphs, candidate filtering, formulation benchmarks, routing.
4. School setup/import/pattern editors/views/locks/repair/projection/export/accessibility.
5. Exhaustive school QA, all nine scenarios, performance, practitioner/usability review.
6. Explainability improvements.
7. Portfolio/resource/diversity improvements.
8. Separate Nuxt site.

### Later independent waves

9. Proven-structure algorithms.
10. Individually gated solver adapters.
11. WASM SDK, manager/catalog, conformance kit.
12. Authenticated service/collaboration design.
13. Enterprise adapters.
14. Selected official domains.
15. AI/import improvements.
16. Optional diagnostics.
17. Additional targets/distribution.
18. Optional hosted services.

## Tests and acceptance

Every branch inherits Phase-12 definitions of done and adds backward compatibility/migrations; typed unknown-newer rejection; independent verifier coverage; cancellation/crash/malformed/resource/failure tests; exact-artifact license/SBOM/notices/provenance; property/differential/metamorphic/fuzz/benchmark/E2E/accessibility/usability evidence; accurate capability/stability/privacy labels; docs/ADRs/ownership/recovery; and offline/local behavior when optional components fail.

## Risks and failure handling

| Failure | Required response |
|---|---|
| School semantics ambiguous | Obtain practitioner examples, specify semantics, add verifier fixtures before implementation. |
| School model explodes | Pre-filter, inspect formulation/model summaries, enforce budgets; do not freeze UI. |
| Unnecessary individual data | Prefer cohorts, minimize fields, document purpose/retention/export. |
| Portfolio overspends or compares unlike scores | Enforce one budget and authoritative vectors; disable unsafe combinations. |
| Backend cannot be verified/isolated | Stop branch and write ADR; do not ship/route. |
| License uncertain | Do not bundle; inspect exact components. |
| Native plug-in requested | Use constrained WASM or compiled official code; reject in-process untrusted loading. |
| Sandbox needs ambient authority | Redesign a narrow mediated capability with disclosure/tests; absent by default. |
| Service reuses Tauri IPC | Reject; design separate authenticated/authorized/audited API. |
| Hosted feature becomes local dependency | Restore local/offline parity or do not ship. |
| AI/import needs arbitrary file/shell | Redesign typed scoped tools. |
| Telemetry contains identifiers | Block collection/release and redesign. |
| Target lacks clean-machine evidence | Do not advertise it. |
| Correctness depends on undocumented behavior | Obtain official conformance evidence or remove capability. |

## Exit gates

The immediate milestone exits only when the complete school contract and nine scenarios pass; explanation/portfolio features preserve verification, certainty, and resource invariants; diversity is explicit and accurately scored; the Nuxt site is separate and accessible; formats migrate safely; and exact artifacts pass Phase-12 gates.

A later branch exits only when its contract, tests, security/privacy/license review, migration/compatibility, accessibility/usability, packaging/operations, ownership/docs, and rollback/removal path are complete. One branch never makes another an implicit commitment.

## Deferred and non-goals

- Marketplace, server, telemetry, hosted service, or proprietary solver is not prerequisite for school.
- MiniZinc text never becomes scenario/Planning-IR authority.
- Graph heuristics never become unverified final solutions.
- Cross-solver decomposition is not mislabeled portfolio execution.
- Third-party packs never use untrusted native in-process ABI.
- Server mode never directly exposes Tauri IPC.
- Vendor fields never pollute core domain contracts.
- Hosted/paid services never remove required local open-source functionality.

## Assumption and version gates

- Direct evidence in [the assumptions ledger](assumptions.md) controls conflicts.
- School defaults, pattern/fairness weights, calendar/DST/projection semantics, sensitive/accessibility metadata, and legal wording require practitioner/usability review.
- OR-Tools 9.15 retains platform, flags, source/proto hashes, linkage, SBOM/license, benchmark, callback, and assumption-core gates.
- Pumpkin 0.5.0 remains gated on supported primitives, dedicated-thread ownership, cooperative cancellation/time limits, verifier, and benchmarks.
- HiGHS 1.15.1, SCIP 10.0.3, and MiniZinc 2.10.0 are discovery values, not automatic pins; reverify and inspect exact licenses at branch start.
- WASM ABI/signatures/resources/UI/catalog/migrations require an ADR.
- Provider APIs/local endpoints are mutable; reverify official contracts, OAuth, streaming, schemas, and keyring behavior per release.
- Final CLI name must precede stable `optimizer serve` docs. Auth/token/binding/TLS/audit/concurrency remain service gates.
- Telemetry needs separately approved schema, consent/preview/deletion/retention and privacy review.
- Linux arm64, Windows arm64, rpm/repositories, universal macOS, and new endpoints require the full target gate.
