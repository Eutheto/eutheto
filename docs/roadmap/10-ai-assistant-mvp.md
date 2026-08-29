# Phase 10 — Optional AI Assistant MVP

## Outcome

Deliver an optional, provider-neutral assistant for natural-language configuration and deterministic-evidence explanation. It can inspect a bounded scenario through typed reads and assemble typed command proposals, but it is never the solver, verifier, policy authority, or source of truth. Every AI-assisted write is previewed as a structured diff, validated at a specific base revision, explicitly applied by the user as one undoable command batch, and rejected or revalidated when stale.

Eutheto remains fully usable with AI disabled: workforce and seating creation, imports, rules, validation, optimization, deterministic explanations, repair, comparison, and export all have equivalent non-AI interfaces. Provider/network failure or a fully compromised model response cannot expand tool authority or mutate scenario state by itself.

## Source coverage

This phase is the implementation source of truth for blueprint Section 23; the AI-relevant controls of Section 24; AI placement and failure states in Section 22; Phase 10; AI Definition of Done 33.5; relevant core/desktop definitions of done; CLI/API Appendix C–D; current-provider assumptions from Appendix K.6/J.5; backlog `AI-001` through `AI-005` plus the AI-relevant parts of `SEC-001` from Appendix I; the AI critical-path exclusion in [Performance and Solver UX Targets](performance-and-solver-ux-targets.md); and the no-AI-authority boundary in [Portable Data, Backup, and Result Sharing](portable-data-backup-and-sharing.md).

Project-wide principles and ADRs are in [README.md](README.md); dated API/version evidence and unresolved conformance gates are in [assumptions.md](assumptions.md). Typed domain commands come from [Phase 02](02-domain-pack-and-planning-ir-contracts.md), deterministic verification/evidence from [Phase 04](04-independent-verifier-and-explanations.md), workforce tools from [Phase 07](07-workforce-solving-results-repair-and-export.md), and seating tools from [Phase 09](09-seating-domain-and-venue-experience.md). The assistant must not depend on the experimental backend in [Phase 08](08-pumpkin-backend-and-router.md).

Section 24 ownership is intentionally split. This phase owns only AI-relevant capability/CSP, credential, provider-network, parser, redaction, and dependency-change controls. Bundled OR-Tools worker manifest/hash/location integrity belongs to [Phase 03](03-ortools-worker-vertical-slice.md). Signed updater metadata/channel separation and project dependency/lockfile/action/SBOM policy belong to [Phase 11](11-public-mvp-packaging-and-documentation.md). Cross-cutting security and public-release review gates belong to [Phase 12](12-stabilization-and-public-release-gate.md). Future plugin capability sandboxing belongs to [Phase 13](13-post-mvp-roadmap.md).

## Dependencies and entry conditions

- Revisioned scenario commands/batches, strict command schemas, validation, changesets, undo/redo, history, and stale-revision errors exist.
- Domain-pack command catalogs expose stable namespaced IDs, JSON Schemas, localized descriptions, risk/reversibility/proposal metadata, and examples.
- Typed queries expose bounded scenario, validation, entity, rule, solution, and deterministic evidence views.
- Workforce and seating each have complete deterministic non-AI editors and explanations.
- The Rust application owns scenarios, solve jobs, solutions, AI conversations, credentials references, and proposal state. Vue remains a typed view/controller.
- Minimum Tauri capabilities, restrictive CSP, structured redacted logging, safe parser limits, and application error taxonomy are in place.

## Decisions and invariants

1. **AI is optional.** No account, provider, key, consent, or model is needed for core use. Disabling AI removes assistant affordances without changing deterministic functions.
2. **AI never decides truth.** It cannot construct backend source, bypass validation, invent user facts, authoritatively decide legal/industry requirements, accept its writes, verify a backend candidate, or replace deterministic evidence.
3. **No ambient authority.** It cannot read arbitrary files, execute shell/code/SQL, fetch model-produced URLs, access credentials, export/delete, or invoke non-allowlisted operations.
4. **Rust enforces policy.** Provider calls, tool allowlisting, strict argument parsing, limits, command validation, proposal assembly, revision checks, and credential handling live in Rust. System prompts are defense in depth, not the boundary.
5. **Reads and writes differ.** Bounded safe reads may execute automatically in the current scenario. Writes only append typed operations to an in-progress proposal; no model tool call mutates.
6. **Scenario-write preview/apply is mandatory.** Validate the whole scenario-command proposal, render a structured non-chat diff, and require explicit user Apply. Apply is one command batch and one-step Undo. Separately proposed application actions use their own typed preview and confirmation path.
7. **Revision safety.** A proposal binds to `scenario_id` and `base_revision`. A changed scenario requires command-by-command revalidation/rebase and a new preview; stale proposals never auto-apply.
8. **Provider-neutral core.** Internal messages, tools, stream events, errors, usage, and completions do not expose provider response types.
9. **Deterministic evidence remains visible.** AI may paraphrase facts, but the expandable evidence view is authoritative; missing evidence or unfinished counterfactuals are disclosed.
10. **Privacy is explicit and local-first.** Configuration alone sends nothing. Each use scopes context to the task, identifies destination/locality, applies user redaction settings, and never logs secrets or full payloads by default.
11. **Provider errors preserve state.** No hidden retry on authentication/billing errors; partial proposals are unapplied unless the user explicitly reviews a valid smaller batch.
12. **Verified means configured rules passed.** AI suggestions are not legal/professional advice and cannot turn presets into compliance claims.
13. **Adapter enablement is conformance-gated.** Every AI adapter enabled in a build must pass its complete recorded conformance suite for the exact implemented contract. An adapter that is excluded, unfinished, stale, or failing conformance stays disabled and exposes an accurate unavailable state and reason; it cannot block the non-AI core release.
14. **AI is outside the deterministic result critical path.** A provider call cannot be required to start, solve, verify, score, persist, render, or export an accepted plan. The verified result appears before optional AI paraphrase; AI/provider timing is recorded separately and cannot be used to qualify core solver latency.
15. **AI has no portability or publication authority.** It cannot inspect raw bundle contents, initiate/apply import or restore, create backups/exports/share reports, select privacy fields, or include conversations/provider payloads in portable or Share Result data. Deterministic non-AI services own every such preview and action.

## Provider-neutral architecture

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn descriptor(&self) -> &AiProviderDescriptor;
    async fn list_models(
        &self,
        auth: &CredentialRef,
    ) -> Result<Vec<AiModelDescriptor>, AiProviderError>;
    async fn stream_turn(
        &self,
        request: AiTurnRequest,
        sink: AiEventSink,
        cancellation: CancellationToken,
    ) -> Result<AiTurnCompletion, AiProviderError>;
}
```

`AiProviderDescriptor` records stable provider ID, display name, API family/version, default and configurable endpoints, remote/local classification, authentication modes, tool/stream/strict-schema capabilities, model-list capability, limits, warnings, and build/runtime availability (`Enabled`, `Disabled`, or `Unavailable`) with a stable reason. A build-excluded adapter remains accurately discoverable as unavailable rather than appearing enabled. `AiModelDescriptor` includes stable provider/model reference and only capabilities the adapter has established.

Adapters translate the internal message/tool/event model to native HTTP requests and normalize native stream deltas, tool calls, finish reasons, refusals, usage, rate-limit details, and typed errors. Provider-native DTOs remain inside adapter crates/modules. Use bounded HTTP clients with connect/read/total timeouts, cancellation, redirect policy, TLS validation, body limits, decompression limits, and redacted tracing.

### Current provider contracts to implement and fixture

Provider APIs are mutable HTTP contracts rather than lockfile packages. The following contracts were verified on 2026-08-29 and must be rechecked against official documentation when adapters are implemented or updated:

| Adapter | Current request contract | Authentication/version headers | Tool-result correlation and caveats |
|---|---|---|---|
| OpenAI | `POST https://api.openai.com/v1/responses` with `store: false` | `Authorization: Bearer <key>`; optional provider-supported project/organization headers only when explicitly configured | Responses emits flat `function_call` items and accepts `function_call_output` linked by `call_id`; use strict JSON Schema with `additionalProperties: false` where supported. Do not implement new work against obsolete package assumptions. |
| Anthropic | `POST https://api.anthropic.com/v1/messages` | `x-api-key: <key>` and required `anthropic-version: 2023-06-01`; add `anthropic-beta` only for a separately reviewed capability | Standard tool use emits `tool_use`; follow-up `tool_result` references `tool_use_id` and immediately follows the tool-use turn. Stable custom tools do not require a beta header. |
| Google Gemini | `POST https://generativelanguage.googleapis.com/v1beta/interactions` with `store: false` | `x-goog-api-key: <key>` for API-key mode; never place keys in logged/query URLs | The Interactions API is GA, while the current official REST quickstart uses this `v1beta` endpoint; do not infer a `/v1` REST path from GA status. Interactions uses `steps[]`, `function_call`, and `function_result` correlated by `call_id`. Reconfirm the exact endpoint, schema, streaming, and `auto`/`any`/`none`/`validated` modes against current docs; do not use deprecated `google-generativeai` assumptions. |
| OpenAI-compatible | User-confirmed base URL plus advertised `/v1/responses` or `/v1/chat/completions` | User-selected no-auth or header credential; destination shown before attaching secret | Detect and persist capability profile. Chat Completions nests function tools/calls differently and many local servers lack strict schema, streaming, parallel calls, or Responses support. Never assume parity from the label. |

Provider requests are stateless by default. OpenAI Responses and Gemini Interactions adapters must send `store: false` on every turn and must not depend on provider-retained response or interaction IDs. A future provider-retention mode is excluded unless it is separately disclosed, explicitly consented, and implements provider-side deletion so eutheto can honor deletion end to end.

Local convenience presets may populate, but never silently contact, current endpoints such as Ollama at `http://localhost:11434/v1` and LM Studio at `http://localhost:1234/v1`. They remain user-installed services, not eutheto dependencies or pinned runtime guarantees. As of the evidence date, the tested preset references are Ollama 0.33.2 and LM Studio 0.4.23; actual capability is model/server-specific.

For OpenAI-compatible services, probe only after explicit user action and without a credential where possible. Present a capability matrix for Responses versus Chat Completions, streaming, tool calls, strict schema, parallel tool calls, model listing, no-auth, and context limits. Unsupported functionality is disabled or warned before a turn, not discovered after an unsafe write proposal.
Every adapter that is enabled in the build must pass recorded request/response/stream conformance for its exact endpoint, authentication, statelessness, tool correlation, capability claims, normalization, and error behavior before enablement and again when implemented or updated. Adapters omitted from the build or failing that gate are `Unavailable` or `Disabled` with a stable user-visible reason in the provider catalog and settings; their absence cannot fail the core release or impair deterministic workflows.

## Authentication and credential lifecycle

MVP authentication supports API keys, optional custom base URL, provider-specific project/organization identifiers when required, and explicitly enabled no-auth local endpoints. OAuth is excluded unless a provider later publishes a desktop third-party authorization flow appropriate to eutheto. Never reuse consumer chat subscriptions, scrape browser cookies, or imply subscription/API billing equivalence.

Store credential bytes only in the OS credential store through the pinned reviewed `keyring` Rust crate. SQLite and settings persist an opaque `CredentialRef` plus nonsecret provider metadata. Credential entry is mandatory in a Rust/native-owned secure surface outside the webview; Vue may invoke `ai_store_credential` only with nonsecret provider/entry intent, and the Vue → Rust IPC request must never contain credential bytes. The native surface passes the value directly to Rust, which stores it immediately, best-effort zeroizes/drops transient buffers, and returns only opaque reference/status. No Vue/HTML credential input, JavaScript object, DOM state, Pinia/conversation state, analytics, diagnostics, URL, persisted form state, command response, query, or event may contain the value.

Provide replace and delete. Deletion removes the OS-store entry and marks dependent configuration invalid without leaking whether unrelated entries exist. On Linux, handle absent/locked Secret Service implementations with a typed setup/recovery error; never fall back to plaintext storage. CI uses a fake credential store.

Redact authorization, `x-api-key`, `x-goog-api-key`, query keys, provider payload fields, and credential references where linkage is sensitive. Do not place secrets in panic reports, support bundles, crash dumps under application control, child-process environments, or logs.

## Network and endpoint boundary

All provider HTTP originates in Rust, never from Vue or an unrestricted webview HTTP plugin. Webview CSP disables network by default. This centralizes secrets, destination policy, redaction, timeouts, cancellation, and body limits.

For a custom base URL:

1. require explicit user entry and confirmation;
2. parse and canonicalize it as a URL, rejecting embedded username/password and fragments;
3. reject unsafe schemes; default to HTTPS except explicitly acknowledged localhost/private local endpoints;
4. show exact destination host and remote/local classification whenever a credential is attached;
5. resolve/validate the destination under the application’s SSRF/private-address policy;
6. prevent credential-bearing redirects to an unrelated host or downgraded scheme; require new confirmation rather than forwarding;
7. apply response, decompressed-body, header, stream-event, and duration limits;
8. keep web/search/fetch tools absent from the MVP, so model-produced URLs are never executed.

A malicious endpoint can return arbitrary bytes/tool calls. Parse it under limits and feed every tool request through the same allowlist/schema/policy pipeline.

## Conversation and context model

```rust
pub struct AiTurnRequest {
    pub conversation_id: ConversationId,
    pub system_policy_version: String,
    pub model: AiModelRef,
    pub messages: Vec<AiMessage>,
    pub tools: Vec<AiToolDefinition>,
    pub context: ScenarioContextPacket,
    pub limits: AiTurnLimits,
}
```

Persist conversations locally with stable conversation/turn/message IDs, provider/model references, scenario ID and revision context, policy/schema versions, redacted normalized content blocks, tool call/result records, proposal IDs, completion/failure/cancellation status, and provider-neutral usage where available. Do not persist credentials or raw authorization/provider transport records. Retention obeys user settings; delete conversations transactionally and make their removal visible.

`ScenarioContextPacket` is assembled from typed queries for the exact task. Include only required fields and label source/revision/sensitivity. Large scenarios are summarized and paged through bounded tools rather than copied wholesale. Stable aliases may replace person/guest names where practical; notes are excluded unless explicitly enabled. Imported notes, CSV cells, scenario descriptions, and all external text are marked untrusted data, never concatenated as policy instructions.

`AiTurnLimits` bounds context bytes/tokens, output, tool calls, tool rounds, entity/page counts, per-tool response size, total wall time, and provider response body. Enforce limits locally even when a provider advertises more.

## Exhaustive MVP tool catalog

### Read tools

Safe, bounded reads may execute automatically within the current scenario and revision:

- `scenario.get_summary`;
- `scenario.get_validation`;
- `entities.search`;
- `rules.list`;
- `solution.get_summary`;
- `solution.get_assignment`;
- `solution.get_rule_evidence`;
- `history.get_recent_changes`.

Each read schema requires scenario/revision scope where applicable, stable cursor/page size, filters from a typed allowlist, and response limits. Reads cannot enumerate the filesystem, raw bundle/report data, database, credentials, other projects, hidden notes, or arbitrary history.

### Scenario-write proposal tools

These create scenario-command proposal operations and never mutate immediately:

- `workforce.add_person`;
- `workforce.set_eligibility`;
- `workforce.add_availability`;
- `workforce.add_minimum_rest_rule`;
- `workforce.add_coverage_rule`;
- `workforce.set_fairness_policy`;
- `seating.add_guest`;
- `seating.add_relationship`;
- `seating.set_minimum_distance_rule`;
- `seating.lock_guest_to_seat`;
- `scenario.apply_import_mapping`;

Schemas are generated from the domain command catalog and use domain language, constraints, examples, stable IDs, and `additionalProperties: false` where supported internally. The scenario-write catalog can expand only through the full AI-capability definition of done and an existing deterministic non-AI command.

`solve.propose_start` is the only MVP application-action proposal. It carries a typed `SolveStartRequest`, never a `ScenarioCommand`, and requires a separate structured preview and explicit confirmation. Rust checks its scenario revision and pre-solve validation again immediately before dispatching it to `solve_start`; it never enters `Vec<ScenarioCommand>` or `scenario_apply_batch`.

Export, backup, import/restore, Share Result/privacy selection, delete, credential, arbitrary settings, update, filesystem, URL fetch, shell, code, and SQL operations are not AI tools in the MVP. AI conversations and provider request/response content are excluded by default from editable bundles, full backups, diagnostics, and share reports; no model can override those code-owned policies.

## Tool-call validation and policy loop

For every requested call, in order:

1. match an exact allowlisted tool ID and enabled domain capability;
2. parse against the internal strict schema and deny unknown fields where appropriate;
3. enforce payload, array, string, nesting, entity, count, response, round, token, and time limits;
4. require current scenario/revision scope and resolve entity references by stable ID;
5. if the model supplied a fuzzy name, return typed disambiguation candidates and require an explicit stable-ID follow-up;
6. authorize read/write risk under code-owned policy;
7. run normal command validation for writes;
8. execute a bounded read, append a typed `ScenarioCommand` to a scenario-write proposal, or record a separately typed `SolveStartRequest` application-action proposal; neither proposal path mutates scenario or application state;
9. return a typed, bounded result/error to the provider loop;
10. stop at configured tool-round/turn limits or cancellation.

Never execute model-produced URL, path, source code, SQL, shell text, template, regex with unbounded behavior, or raw solver parameters. Do not deserialize model output directly into internal privileged types without the public tool schema/policy path.

Provider `strict` modes improve generation but are not trusted validation. Internal parsing and policy apply identically to OpenAI, Anthropic, Gemini, local endpoints, recorded fixtures, and fake provider.

## Proposal, preview, apply, and undo

```rust
pub enum AiProposal {
    ScenarioBatch(AiScenarioProposal),
    SolveStart(AiSolveStartProposal),
}

pub struct AiScenarioProposal {
    pub id: AiProposalId,
    pub scenario_id: ScenarioId,
    pub base_revision: u64,
    pub title: String,
    pub rationale: String,
    pub commands: Vec<ScenarioCommand>,
    pub validation_preview: ValidationReport,
    pub diff: ChangeSet,
    pub status: AiProposalStatus,
}

pub struct AiSolveStartProposal {
    pub id: AiProposalId,
    pub scenario_id: ScenarioId,
    pub base_revision: u64,
    pub title: String,
    pub rationale: String,
    pub request: SolveStartRequest,
    pub validation_preview: ValidationReport,
    pub status: AiProposalStatus,
}
```

Proposal assembly is transactional in memory/application storage. For a scenario batch, validate the command sequence against a temporary copy in order, then validate the resulting whole scenario. Render title/rationale as untrusted model text and render the authoritative command-derived structured diff outside the chat. Show additions, edits, deletions, rule strength/scope, affected entities, warnings/errors, validation changes, and exact base/current revisions.

Apply is enabled only for a valid complete proposal at the current revision. A `ScenarioBatch` invokes the same `scenario_apply_batch` service as non-AI edits, records AI provenance without granting special authority, commits once, and creates one undo unit. A `SolveStart` renders a separate action preview with mode, limits, warnings, validation, and revision; after its own explicit confirmation, Rust rechecks the revision and pre-solve validation and dispatches the typed request to `solve_start`. It does not mutate the scenario and is not an undoable command batch. Reject changes status without touching the scenario or starting a job.

If the revision changed, mark the proposal stale. `ai_rebase_proposal` requires the proposal ID and expected current revision; Rust confirms that revision, reruns every scenario command against the current document, resolves conflicts through normal typed errors/disambiguation, recomputes full validation and the structured diff, and returns a fresh preview requiring a fresh Apply. A stale solve-start proposal is likewise revalidated against the current revision and returns a fresh action preview. Stale proposals never apply or start a job.

A partial scenario batch after provider interruption is visibly incomplete and non-applicable by default. The user may explicitly choose a valid smaller batch only after it is independently revalidated and previewed as a new complete proposal. An interrupted solve-start proposal cannot start a job.

## Prompt-injection and untrusted-content handling

Treat imported notes, CSV values, scenario descriptions, guest/person names, external provider text, and old conversations as data. Context serialization separates system policy, trusted typed facts, and untrusted strings. Escape/restrict rich text, disable raw HTML, and never interpolate untrusted content into tool definitions or policy.

Policy is enforced outside prompts: even a model fully following malicious embedded instructions can request only allowlisted typed tools, within scenario and resource scope; reads remain bounded; writes remain unapplied proposals. Add an injection corpus covering instructions to reveal keys, browse files, execute shell/SQL, fetch URLs, disable validation, make a Required rule soft, apply without confirmation, exfiltrate other projects, loop tools, or hide a diff.

The assistant may quote/describe suspicious imported text only when needed and clearly label it as data. It never treats imported claims about laws, eligibility, or relationships as facts without user confirmation through a typed proposal.

## Deterministic-evidence explanations

For a question such as why Jones received Tuesday overnight, the context contains structured evidence—not a raw solver narrative—including eligible assignment facts, unavailable alternatives and source rule IDs, rest conflict hours, preference contributions, fairness/stability deltas, proof/status, scenario/solution revision, and certainty `deterministic-evidence`.

AI may turn this into plain language, but `View evidence` exposes the original structured facts and deterministic explanation. If evidence is incomplete, multiple equivalent optima exist, or a counterfactual timed out, say so. Never call a heuristic narrative proof, invent unique causality, or claim optimality from feasibility.

Assistant actions can request the existing bounded counterfactual service only through a typed read/explicit proposal flow as designed; its result must independently verify before use.

## Privacy, user control, cost, and retention

Before first AI use, disclose:

- exact provider and destination host;
- local versus remote endpoint;
- that selected scenario excerpts can contain names and schedules/seating relationships;
- credential storage in the OS keyring;
- optimization and deterministic explanations work without AI;
- provider API usage is distinct from consumer subscriptions;
- AI suggestions are not legal/professional advice.

Settings provide AI globally disabled; global or per-project provider/model; alias substitution for names where practical; notes included/excluded; conversation retention; credential deletion; conversation deletion; and local-only-provider mode. Configuration does not transmit data. Per-turn context preview/size disclosure and confirmation apply before unusually large sends.

Show provider-neutral input/output/total usage estimates when adapters report them, but do not promise exact prices. Bound context, output, tool rounds, and total time; allow per-turn cancellation; optionally retain a local usage log without secrets/content; never hide automatic retries after authentication or billing failure. Any retry policy is bounded, visible, cancellation-aware, and restricted to explicitly transient failures.

Provider-side storage is disabled by default independently of local conversation retention: OpenAI Responses and Gemini Interactions always send `store: false`. Deleting a local conversation therefore does not depend on provider-retained state. Any future provider-retention mode must disclose its retention period and data destination, obtain separate consent, expose provider deletion, and complete that deletion when the user requests it; otherwise the mode cannot be enabled.

No required telemetry ships. If diagnostics are ever added, they are explicit opt-in, schema/code-public, contain no scenario/chat/name data, are inspectable/deletable, and do not affect functionality.

## Failure behavior

Handle all of these as typed recoverable states while preserving scenario integrity and the user’s original message:

- network/connect/read/total timeout;
- rate limit, including bounded provider retry metadata;
- invalid/revoked key;
- model removed or unavailable;
- malformed, unknown, oversized, or schema-invalid tool call;
- content refusal;
- endpoint/API incompatibility;
- redirect/destination policy rejection;
- response/body/decompression limit;
- stream interruption or invalid event ordering;
- cancellation race;
- credential store absent/locked/unavailable;
- proposal command or whole-proposal validation failure;
- stale revision/rebase conflict;
- conversation persistence failure.

Preserve completed safe reads with clear status. Do not apply any proposal on failure. Do not endlessly retry auth, billing, refusal, invalid schema, or incompatibility. Provide retry/reconfigure/switch provider/copy sanitized diagnostic actions. The core application and current scenario continue normally when the assistant is unavailable.

## Security and trust-boundary requirements

- Trust crossings are Vue ↔ typed Tauri IPC, Rust ↔ external HTTPS/local endpoint, and Rust ↔ strict imported-data parsers; treat every crossing as untrusted. Credential bytes never cross the Vue IPC boundary in either direction: a nonsecret Vue request opens Rust/native-owned secure entry, and only opaque reference/status crosses back.
- Tauri capabilities grant no shell, no broad filesystem, no unrestricted webview HTTP; file access is dialog/scoped; credential and AI commands are explicitly allowlisted in reviewed capabilities.
- Restrictive CSP permits bundled scripts/styles, no arbitrary remote scripts/eval, safe local/blob images only, and no default webview network. Sanitize restricted Markdown with raw HTML disabled.
- Parser limits include UTF-8/explicit CSV encoding review, streaming rows, file/row/nesting/allocation/image/ZIP limits, no macros/active content/templates.
- Structured logs include lifecycle, request/turn IDs, statuses, durations, provider/model/version metadata, and bounded error codes—not scenario documents, names, notes, chat content, request/response payloads, or keys by default. Rotate and cap logs; preview support bundles.
- Dependency changes to HTTP/TLS, parsers, keyring, Tauri permissions, redaction, and provider adapters require security review, pinned lockfiles, license/advisory checks, and SBOM coverage.
- Compliance UI states that presets are starting templates and users must verify laws/contracts/policy. A verified solution satisfies configured rules only.

## Application API and events

Required Tauri commands/queries:

- `ai_get_provider_catalog`;
- `ai_get_configuration`;
- `ai_store_credential` (nonsecret entry intent only; Rust opens and owns the native secure-entry surface);
- `ai_delete_credential`;
- `ai_test_provider`;
- `ai_list_models`;
- `ai_list_conversations`;
- `ai_create_conversation`;
- `ai_get_conversation`;
- `ai_send_turn`;
- `ai_cancel_turn`;
- `ai_get_proposal`;
- `ai_rebase_proposal`;
- `ai_apply_proposal`;
- `ai_reject_proposal`;
- `ai_delete_conversation`.

Mutating operations include request ID and expected revision where applicable. `ai_rebase_proposal` takes proposal ID plus expected current revision and returns the recomputed validation/diff or action preview at that revision. Responses include schema version, warnings, and current revision. Errors use stable category/code, retryability, safe field errors/details, and diagnostic ID without raw provider bodies/backtraces.

Events are `ai://stream`, `ai://proposal-ready`, and `ai://completed`, with event version, timestamp, request/turn/conversation/scenario IDs and revision where applicable. Normalize text deltas, tool-status summaries, usage, refusal, and completion without sending secrets/provider-private payloads to Vue. Only `apps/desktop/src/api` invokes Tauri/event APIs.

The assistant is a collapsible side panel, never the only interface. It can answer typed current-scenario questions, propose setup/rule changes, guide imports, propose validation/optimization with confirmation, and paraphrase deterministic results. Proposal diff and Apply/Reject are outside model-authored chat. Design AI-disabled, unconfigured, credential error, offline, loading, streaming, cancelled, refusal, rate-limited, incompatible endpoint, stale proposal, validation failure, and partial stream states; no indefinite spinner.

AI-specific CLI commands are optional to expose in the MVP command catalog; if exposed they use the same application service, explicit input paths/provider configuration, JSON envelopes, stderr diagnostics, no secrets, and documented AI provider exit code. The CLI must never make AI necessary for any scenario command.

## Ordered work packages

1. **AI-001 — Provider-neutral core and fake:** internal DTOs/events/errors, descriptor/capability model, limits, cancellation, fake credential/provider, deterministic scripted turns.
2. **AI-002a — Credential/network boundary:** mandatory Rust/native-owned secure entry, nonsecret request and opaque reference/status response IPC, OS keyring storage/delete/replace, CSP/capabilities, bounded HTTP, redirect/destination/redaction policy, Linux unavailable-store UX, and request/response/DOM secret-canary tests.
3. **AI-002b — Current adapters:** OpenAI Responses, Anthropic Messages, Gemini Interactions, and capability-detected OpenAI-compatible/local presets with recorded conformance fixtures.
4. **AI-003 — Context and reads:** scoped packet builder, aliases/note policy, paging/summaries, eight allowlisted read tools, limits and untrusted-data labeling.
5. **AI-004 — Proposals and application actions:** generated schemas for eleven scenario-write tools plus the separately typed solve-start application action; validation loop; disambiguation; whole-batch diff preview; solve-action preview; typed stale rebase; batch apply/reject and one-step undo; revision-checked `solve_start` dispatch.
6. **AI-005a — Evidence:** structured deterministic explanation packet, incomplete/counterfactual disclosure, evidence inspector.
7. **AI-005b — Desktop UX/settings:** first-use disclosure, provider/model/endpoint configuration, conversation retention/deletion, privacy/context controls, usage/cancellation, all failure/empty states, accessible assistant panel.
8. **Security hardening:** injection corpus, malicious endpoint/redirect/body tests, redaction/support-bundle checks, capability/CSP review, dependency/license audit.
9. **Provider and cross-domain acceptance:** representative workforce and seating conversational setup using real typed proposals plus complete AI-disabled regression.

## Tests and acceptance

### Provider conformance

Recorded secret-redacted request/response/stream fixtures cover current endpoint, required stateless-storage flags, headers, tool schema, multiple calls, call/result IDs, usage, finish/refusal reasons, cancellation, malformed ordering, rate limits, and errors for every enabled adapter and compatible endpoint shape. Live calls are optional manual conformance, never CI’s sole evidence. Contract updates require fixture/schema/version review, and no adapter may remain enabled after its applicable recorded conformance fails.

For enabled built-in adapters, test Anthropic `tool_use`/immediate `tool_result`; OpenAI flat Responses calls/output, strict-schema subset, and `store: false`; Gemini current `/v1beta/interactions`, `store: false`, step/call IDs, and function results; and local capability warnings when strict/stream/parallel/Responses are absent. Verify no provider DTO escapes adapter modules and no default request depends on provider-retained state. A listed adapter that is build-excluded, disabled, or fails conformance must report `Unavailable`/`Disabled` with its stable reason in catalog, configuration, and assistant UI tests and cannot fail AI-disabled/core-release acceptance.

### Policy, proposal, and security

- exact allowlist and strict schemas, unknown field denial, every size/count/nesting/round/time boundary;
- fuzzy entity disambiguation and stable-ID-only execution;
- safe read scope/paging and cross-project/secret/file denial;
- every scenario write requires a proposal, whole validation, structured diff preview, explicit apply through `scenario_apply_batch`, and one-step undo;
- solve start remains a separately confirmed typed application action, is revision/pre-solve-validation checked, dispatches only to `solve_start`, and never enters a scenario-command vector or batch;
- a stale proposal cannot apply or start a job; typed rebase with proposal/current-revision input produces recomputed validation and a fresh diff/action preview;
- provider interruption never applies a partial proposal;
- model URLs, paths, code, SQL, and shell are inert;
- injection corpus in CSV, notes, descriptions, names, old messages, and endpoint responses cannot expand capability;
- malicious redirect cannot receive credentials; unsafe schemes/URL credentials/private-policy violations reject;
- native-entry canary tests inspect the serialized Vue → Rust `ai_store_credential` request and prove it contains only nonsecret provider/entry intent; paired Rust → Vue command responses, events, and errors contain only opaque reference/status and safe metadata;
- instrumented webview tests prove the canary secret never enters JavaScript objects, Vue/Pinia state, DOM values/attributes/text, persistence, logs, support bundles, fixtures, errors, or usage records; credential replacement/deletion and unavailable/locked keyring states preserve the same boundary;
- cancellation and stream interruption races;
- large context/output/body/decompression/tool-loop limits;
- conversation retention/deletion and persistence failure;
- OpenAI Responses and Gemini Interactions default fixtures always set `store: false`; any future consented provider-retention mode proves disclosure, provider deletion, and end-to-end delete behavior;
- deterministic evidence grounding, incomplete evidence, and equivalent-optimum language;
- import/restore/export/backup/share/privacy operations are absent from the allowlist, and model text cannot alter portable inclusion, report fields, or output destinations.

### Product acceptance

- complete application functionality with AI globally disabled, no credential, provider offline, and assistant hidden;
- provider failures and malformed calls leave scenario revision/document unchanged;
- slow, cancelled, malformed, or unavailable AI paraphrase never delays or hides an already accepted deterministic result, and provider timing remains separate from solve/verify/render metrics;
- representative workforce flow conversationally proposes a person/eligibility/availability/rest/coverage/fairness configuration and applies only after preview;
- representative seating flow proposes a guest/relationship/minimum-distance/seat-lock configuration and applies only after preview;
- identical operations are possible through deterministic non-AI UI/CLI;
- assistant panel is keyboard operable, correctly labeled, restores focus, announces stream/proposal completion without excessive chatter, supports reduced motion, and never uses color alone;
- users understand provider destination, data scope, key storage, usage uncertainty, AI optionality, and proposal-versus-applied state.

## Risks and failure handling

- **Hallucinated/malicious command:** strict allowlist/schema/validation and non-applying proposal.
- **Prompt injection:** typed context separation plus code-owned policy; corpus tests.
- **Provider API churn:** isolated adapters, dated evidence, recorded conformance, capability discovery, optional AI.
- **Secret leakage:** OS keyring, one-way references, Rust networking, redaction, redirect controls, no payload logs.
- **Malicious custom endpoint:** explicit host confirmation, scheme/redirect/SSRF policy, parser/body/time limits.
- **Stale or partial proposal:** revision binding, full revalidation, fresh preview, no automatic prefix apply.
- **Cost surprise/tool loop:** local hard limits, usage display, confirmation for large context, bounded retries and cancellation.
- **Misleading explanation:** deterministic evidence inspector and explicit uncertainty/proof language.
- **Sensitive scenario disclosure:** task-minimal context, aliases, notes opt-in, remote/local disclosure, configuration sends nothing.
- **Keyring absent on Linux:** actionable error, no plaintext fallback, AI remains disabled/core usable.
- **AI mistaken for compliance authority:** clear preset/advice/verification disclaimers.

Pause and write an ADR if an AI flow needs arbitrary file/code/shell/network authority, a write cannot use normal typed commands, a deterministic non-AI equivalent is absent, a secret must be persisted outside the OS store, security depends only on prompting, or correctness relies on undocumented provider behavior.

## Exit gate

Phase 10 is complete only when:

- the app and CLI core work fully with AI disabled and every AI capability has a deterministic non-AI equivalent;
- provider-neutral internal contracts and fake provider support complete CI behavior without live credentials;
- every AI adapter enabled in a build passes recorded conformance for its exact endpoint/header/statelessness/tool/stream/error contract, including the current Gemini `/v1beta/interactions` endpoint and required OpenAI/Gemini `store: false`; excluded, disabled, stale, or failing adapters expose accurate unavailable status/reasons and do not block the core release;
- credentials persist only as OS-keyring secrets plus opaque references; mandatory Rust/native-owned secure entry keeps secret bytes out of Vue → Rust requests, and only opaque reference/status returns through Rust → Vue responses/events; tests prove secrets never enter webview JavaScript/DOM state or logs/support data;
- every tool is typed, allowlisted, risk-classified, bounded, and validated in Rust;
- scenario writes always produce a command-derived structured diff and require explicit apply as one undoable command batch; solve start uses a separately confirmed, revision-checked typed action dispatched only to `solve_start`;
- stale and partial proposals cannot apply or start a job; `ai_rebase_proposal` recomputes validation and the diff/action preview for the checked current revision;
- imported prompt injection, malicious endpoints, malformed calls, redirects, and tool loops cannot expand capabilities or leak credentials;
- provider timeout/rate/auth/model/refusal/incompatibility/stream/proposal failures preserve scenario state;
- deterministic explanation evidence remains inspectable and incomplete evidence is accurately labeled;
- deterministic accepted results remain available without waiting for AI, provider failures cannot affect their status, and AI latency is excluded from core optimizer performance claims;
- import/restore/export/backup/share behavior and privacy selections remain wholly deterministic and AI-independent; AI conversations/provider payloads do not leak into portable/share output;
- privacy, stateless provider requests, retention, destination, alias/notes, local-only, usage, cancellation, credential/conversation deletion, future consented provider-retention deletion, and all accessibility/error-state UX pass;
- representative workforce and seating rules can be configured conversationally and through equivalent deterministic UI.

## Deferred and non-goals

- OAuth without an official appropriate desktop third-party flow.
- Consumer subscription/cookie reuse, browser automation, arbitrary web browsing/fetch, shell/code/SQL execution, arbitrary local-file reads, autonomous export/delete/credential operations.
- AI-generated solver/backend code, AI verification, autonomous application, legal/compliance decisions, or AI-only workflows.
- Hidden agents, background autonomous edits, unbounded tool loops/retries, hosted eutheto accounts/services, and cross-project memory.
- Domain-guided templates, local-model tuning, richer provider portfolios, semantic retrieval, and hosted collaboration are post-MVP unless separately approved.

## Assumption and version gates

- Provider contracts are mutable. Reverify official docs and refresh recorded fixtures at implementation/update time before enabling an adapter. The 2026-08-29 baselines are OpenAI `POST /v1/responses` with `store: false`; Anthropic `POST /v1/messages` with `anthropic-version: 2023-06-01`; Gemini `POST https://generativelanguage.googleapis.com/v1beta/interactions` with `store: false` (Interactions is GA, but the current official REST quickstart uses `v1beta`); and capability-detected compatible `/v1/responses` or `/v1/chat/completions`.
- Pin reviewed Rust HTTP/TLS/serialization/keyring dependencies in Phase 00. Current verified `keyring` is **4.1.6** and requires Rust compatible with the project’s pinned 1.97.1; recheck Linux Secret Service behavior and fake-store features.
- Validate OpenAI strict-schema subset and Responses/Chat-Completions differences; Anthropic nested schema/`additionalProperties` behavior and immediate tool results; Gemini raw REST endpoint/auth/schema/stream semantics; and local endpoint tool/stream/parallel capability before enabling each adapter or feature.
- Ollama 0.33.2 and LM Studio 0.4.23 are dated preset references, not eutheto dependencies or promises; test the actual configured endpoint/model.
- Exact provider model names are intentionally not hard-coded as enduring defaults. Model catalog, context limits, usage fields, pricing display, and retirement handling come from current provider capability/configuration with safe fallbacks.
- AI provider API churn cannot block core release: every enabled adapter must pass current conformance, while an excluded, failing, or stale adapter is disabled with accurate `Unavailable`/`Disabled` status and reason and deterministic functionality remains complete. CLI name, application ID, hosting organization, signing, and governance contacts remain separate unresolved gates; `.eutheto` remains a proposed extension until the Phase-11 identity ADR closes.
