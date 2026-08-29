<!-- SPDX-License-Identifier: Apache-2.0 -->

# Threat Model

## Scope and status

This threat model covers the approved eutheto architecture and the security foundations established in [Phase 00](roadmap/00-repository-and-reproducible-tooling.md). Phase 00 is repository/tooling work: it does **not** yet implement domain imports, persistence, solver workers, provider access, credential entry, production updating/signing, or a shippable desktop product. Accordingly, this document separates:

- **foundation controls**: binding architecture, policy, pinned-input, least-privilege, generated-contract, and review requirements that Phase 00 can establish; and
- **implementation gates**: executable controls and adversarial tests that must close in the roadmap phase owning the feature before that feature may be represented as available.

An absent future feature reduces current runtime exposure but is not evidence that its future controls work. No deferred feature may be filled with a no-op, mock authority, dummy worker, fake provider, or unsigned “temporary” production path.

Security reports follow the repository [security policy](../SECURITY.md). The hosting organization and security contact are unresolved Phase-00 identity gates; this document does not invent an address or person.

## Security objectives

Protect:

- scenario confidentiality, integrity, availability, revision history, and user-controlled deletion;
- credentials, OAuth tokens, signing/notarization material, and protected release secrets;
- correctness of required-rule verification and authoritative score/explanation evidence;
- the local filesystem, operating-system account, webview/Tauri process, and child-process boundary;
- canonical documents, migrations, generated DTOs/schemas/protocols, and compatibility metadata;
- worker binaries, source/hash/version manifests, release artifacts, checksums, SBOMs, and notices; and
- diagnostics and support artifacts from disclosing scenarios, names, notes, paths, credentials, or provider content.

The product remains local-first, useful without AI, and has no telemetry or network access by default. Availability is bounded by explicit resource limits; correctness and data integrity take precedence over returning a result.

## Adversaries and failure sources

The model includes malicious or malformed imported files, archives, URLs, future third-party pack data/components, worker frames, provider responses, web content, and dependency artifacts. It also includes a compromised dependency or CI action, a tampered release/worker artifact, a curious local user with access to ordinary application files, accidental support-bundle sharing, provider prompt injection, stale/mismatched processes, crashes and partial writes, and ordinary implementation defects.

It does not claim to defend data from an attacker who already fully controls the user's OS account or kernel. OS credential storage and platform signing are relied-on external trust anchors and still require least-privilege application use.

## Trust boundaries

```text
untrusted documents/bundles/URLs ── parser + limits + migration ──┐
                                                                  │
Vue webview ── generated, capability-scoped Tauri API ── Rust application services
                                                                  │
OS credential store ── native credential port ────────────────────┤
                                                                  │
provider/network ── bounded response/tool validation ─────────────┤
                                                                  │
planning IR ── bounded versioned frames ── supervised worker child│
                                                                  │
locked source/dependencies ── build ── digest handoff ── sign/update channel
                                                                  │
                                      transactional SQLite + local files
```

The Rust application-service boundary is authoritative. Vue is an untrusted/presentation client, a solver worker is an untrusted candidate producer, provider output is untrusted proposal data, and imported bytes are untrusted until bounded parsing, migration, and validation complete. Build and sign are separate trust domains.

## Current Phase-00 foundation controls

Phase 00 establishes requirements rather than later-feature runtime claims:

- [architecture boundaries](architecture/dependency-boundaries.md), approved [ADRs](roadmap/README.md#approved-architecture-decisions), and stop conditions prevent privilege or authority from drifting into UI, AI, packs, or backends;
- Rust authoritative state, coarse generated DTOs, Tauri API-layer isolation, minimum capabilities/CSP, and application-manifest registration are mandatory for the minimal shell;
- imported files, archives, worker frames, provider responses, and URLs are classified as untrusted bounded inputs before their owning implementation exists;
- credentials are prohibited from Vue/JavaScript state, SQLite, exports, logs, diagnostics, Nix derivations, repository files, caches, and ordinary IPC; only the future native/OS credential-store path may hold values;
- public formats must be versioned; unknown newer versions fail safely; generated sources are never hand-edited; clean locked generation must prove no drift;
- exact tool/lock inputs, immutable CI action SHAs, read-only CI permissions by default, protected release environments, separate build/sign jobs, and artifact digest verification are repository requirements;
- dependency licenses, advisories, install scripts, parsers, credential/keyring code, worker protocol, updater/signing, cryptography, and Tauri capabilities require explicit policy/review gates;
- hidden global mutable state and normal-crate `unsafe` are forbidden; a narrow `unsafe` exception requires an isolated safety module, documented invariants, dedicated tests, and maintainer review; and
- no updater, signing identity, public extension, reverse-domain ID, hosting organization, contact, provider, worker, or domain feature is presented as complete while its gate is open.

These controls are complete only when Phase 00's own exit evidence passes. Later runtime mitigations below remain gates even if their policy text already exists.

## Threat analysis and required mitigations

### Untrusted scenario documents, bundles, and imports

**Threats.** Path traversal and archive extraction outside the temporary root; symlink/hardlink/device entries; duplicate or Unicode-confusable normalized paths; absolute paths; decompression bombs; excessive bytes, files, records, nesting, strings, or numeric values; checksum substitution; parser differential; malicious migration input; unknown-newer versions; partial database mutation; formula/script content in exported tabular data; and unintended inclusion of credentials, logs, or unrelated files.

**Foundation controls.** [ADR-018](adr/018-public-scenario-representation.md) makes a versioned eutheto document/bundle—not a backend model—the public contract. Formats require centralized limits, compatibility policy, canonical ordering, safe unknown-newer refusal, checked arithmetic, and atomicity. Credentials and unrelated paths are excluded.

**Implementation gate (Phase 01 and owning import phases).** Extract only into a private temporary directory; reject absolute, traversal, backslash-ambiguity, symlink, hardlink, and device entries; detect duplicate normalized paths; enforce compressed/uncompressed/per-file/total/file-count/nesting/record limits before allocation or commit; verify manifest checksums; parse, validate, and migrate fully before one transaction; clean temporary data on every outcome; use parameterized SQL; atomically write exports; safely encode spreadsheet-facing data; and pass adversarial archive/parser/migration/rollback fixtures. No import surface is available before this gate closes.

### Solver worker executable and frames

**Threats.** Tampered or mismatched worker executable/manifest; launching from attacker-controlled `PATH`; malformed, oversized, truncated, repeated, reordered, or unknown frames; protobuf/source mismatch; child crash/hang; stdout/stderr flooding; cancellation escape or orphan process; backend assignment/type confusion; malicious names/paths in diagnostics; and a candidate incorrectly trusted as feasible or optimal.

**Foundation controls.** [ADR-004](adr/004-ortools-worker.md) requires a bundled project-owned child process, matched OR-Tools/protobuf contract, versioned bounded protocol, exact manifest/hash metadata, and no Phase-00 dummy worker. [ADR-007](adr/007-independent-solution-verification.md) denies backend authority.

**Implementation gate (Phases 02–04).** Resolve the worker from the verified application bundle; validate executable and manifest hashes/versions/capabilities before launch; use one supervised process tree per solve; bound every frame, queue, count, string, event rate, timeout, memory/CPU budget, and captured output; reject protocol/state-machine violations and unknown values; make cancellation terminate the process tree; retain human provenance in Rust and send only numeric/stable IDs; project typed values; independently evaluate all required rules and score against the original normalized scenario; quarantine failed verification; and prove all failure modes leave scenario state unchanged. The known OR-Tools assumption-core issue must fail closed rather than guess literal provenance.

### Provider data, prompts, tools, and local endpoints

**Threats.** Prompt injection in scenario/provider content; malicious tool arguments; oversized or malformed streaming responses; URL/redirect abuse and server-side request forgery; provider impersonation; untrusted local endpoint behavior; stale proposals; exfiltration of excess scenario context; a model claiming solver/verifier authority; arbitrary file/shell/code requests; and sensitive content entering logs.

**Foundation controls.** [ADR-013](adr/013-ai-command-boundary.md) limits AI to bounded reads and typed command proposals. [ADR-014](adr/014-provider-authentication.md) permits MVP BYOK/local endpoints and only officially supported suitable OAuth. Network/AI remains optional and off by default.

**Implementation gate (Phase 10).** Use an explicit provider allow/configuration model and reviewed URL policy; apply TLS through the approved platform/client contract; bound redirects, DNS/address classes as required by endpoint policy, response bytes, tokens, tool calls, nesting, time, and concurrency; parse responses as untrusted typed data; expose only least-privilege tools; bind proposals to scenario revision; preview and validate through the normal command transaction; redact provider content and secrets; implement cancellation; and adversarially test prompt/tool injection, malformed streams, redirects, local endpoint attacks, and stale revisions. Provider output can never directly write persistence, files, execute code/shell, route, solve, verify, or become the only explanation.

### Credentials and authentication material

**Threats.** Secrets in Vue/Pinia, IPC responses, SQLite, exports, diagnostics, crash reports, clipboard history, process arguments, environment variables, Nix derivations, caches, tests, or repository files; excessive credential scope; insecure OAuth redirect or refresh; inability to revoke/delete; and secret retention in memory.

**Foundation controls.** [ADR-010](adr/010-local-state-and-credentials.md) assigns values exclusively to the OS credential store behind a Rust/native-owned boundary. Repository and CI policy excludes `.env`, signing material, and ordinary secret injection into PR or derivation contexts. No contact or credential identifier is fabricated.

**Implementation gate (Phases 01 and 10).** Provide a native-owned secure entry surface; pass a secret once to the credential service; store only opaque references/status elsewhere; best-effort zeroize temporary buffers; prevent serialization and debug formatting; use least scopes; implement lookup, replacement, deletion, revocation, expiry, and locked/unavailable-store errors; ensure ordinary IPC never returns values; and test request plus response boundaries, logs, exports, database bytes, and support artifacts for leakage. OAuth is implemented only against verified official provider authorization.

### Tauri, webview, and native capabilities

**Threats.** Cross-site scripting or unsafe HTML gaining native invoke access; broadly registered commands; capability confusion between windows; arbitrary filesystem/shell/open-URL access; remote navigation; CSP weakening; deep-link/file-association injection; frontend dependency compromise; and webview state becoming authoritative.

**Foundation controls.** [ADR-001](adr/001-core-library-and-cli.md) makes Tauri a client; [ADR-012](adr/012-tauri-api-and-generated-dtos.md) requires coarse generated commands, an API-only invoke/event layer, Rust authority, application-manifest registration, minimum capabilities, and CSP. `invoke_handler` alone is not authorization.

**Implementation gate (Phase 00 minimal shell, then each owning feature).** Register every command through the Tauri application manifest and grant it only to the required window/capability; keep invoke/event imports under `apps/desktop/src/api`; deny shell and broad filesystem permissions unless a later reviewed feature requires a narrow scope; prevent remote navigation and unreviewed dynamic content; avoid raw HTML rendering; use a restrictive production CSP; validate command inputs and bound outputs on both sides; review capability diffs; and exercise the packaged Tauri surface. File associations, deep links, updater, and public IDs remain disabled until their identities and handlers pass later gates.

### Local persistence, migrations, and files

**Threats.** Partial writes, stale-revision overwrite, SQL injection, corrupt or unknown-newer databases, migration data loss, concurrent writer races, crash during journal/snapshot update, insecure permissions, backup disclosure, symlink/path confusion, unsafe deletion, and secrets accidentally persisted.

**Foundation controls.** SQLite is local authority, all mutations use typed revision-checked commands, and credentials are structurally separated ([ADR-010](adr/010-local-state-and-credentials.md), [ADR-011](adr/011-command-journal-and-undo.md)). Stable typed IDs, parameterized access, transactional mutation, safe path abstraction, and explicit format versions are binding.

**Implementation gate (Phase 01).** Use the platform application-data location with restrictive feasible permissions; keep one dedicated write owner; enable and test appropriate SQLite integrity/foreign-key/journal settings; parameterize SQL; serialize same-scenario mutation; enforce `expected_revision`; make migrations forward-only and transactional with backups/recovery; refuse unknown-newer databases; use atomic replace for files where supported; handle symlink/race and interrupted-operation cases; and pass injected-failure, migration, concurrency, corruption, backup-retention, deletion, and restart tests. Threats from a fully compromised local account remain outside the application boundary.

### Logs, diagnostics, correctness alarms, and support bundles

**Threats.** Names, notes, scenario contents, prompts, provider responses, credentials, tokens, local paths, environment values, database content, worker stderr, or signing metadata leaking through structured fields, panic/debug formatting, CI artifacts, or user-shared support bundles; unbounded logs exhausting disk; correctness alarms being silently discarded.

**Foundation controls.** Secrets and unsanitized support bundles are prohibited from repository files, logs, caches, exports, and CI artifacts. Worker diagnostics are limited to sanitized versions/counts/statistics. Full solver logs are not persisted by default. No support recipient is invented while contact governance is unresolved.

**Implementation gate (each logging feature; support bundle in its owning later phase).** Use structured allowlisted fields rather than blacklist-only redaction; assign safe diagnostic IDs; bound size/rate/retention; exclude scenario text, credentials, provider content, and paths by construction; sanitize worker stderr; separate quarantined correctness alarms from normal user evidence; make diagnostic export explicit and previewable; generate a manifest of included files/fields; re-scan the final archive for secrets and path/content leakage; and test request/response/error/panic/cancellation paths with seeded canary secrets. A support bundle cannot be marketed as sanitized until those tests pass.

### Dependencies, build, CI, releases, updates, and signing

**Threats.** Floating dependencies or CI actions; malicious install/build scripts; dependency confusion or unreviewed registries; compromised caches; stale generated code; OR-Tools/protobuf mismatch; license contamination; secrets exposed to pull requests; artifact substitution between build and signing; stolen signing keys; updater metadata/key compromise; downgrade, rollback, wrong-channel, or wrong-target update; and SBOM/notice mismatch.

**Foundation controls.** Lockfiles and toolchains are committed; CI actions use full commit SHAs; CI tokens are read-only by default; dependency update workflows use frozen inputs and isolate major migrations; new/changed JavaScript install scripts require exact review and allowlisting; sensitive dependency areas require code-owner review; release environments are protected; build and sign are separate with digest handoff; checksums, SPDX SBOMs, manifests, and notices are required by [ADR-016](adr/016-release-evidence.md). Signing identities and updater choices remain open gates.

**Implementation gate (Phase 00 supply-chain workflows and Phases 11–12 release/update).** Enforce frozen installs and source allowlists; verify dependency licenses/advisories/inventory and generated drift; pin and verify native source archives/protos/manifests; restrict cache write/use by trust level; prevent PR code from accessing secrets; produce reproducible target artifacts where supported; verify artifact digest before signing; keep signing/notarization keys in the selected protected CI/HSM/OS custody; generate and verify checksums, SPDX SBOM, notices, provenance, and solver/version manifests; define signed updater metadata, channel/target/version/rollback policy, expiry/rotation/revocation, and failure recovery; and test tampered, stale, downgraded, cross-target, and partially downloaded updates. No updater is enabled and no production artifact is called signed until identity/custody and clean-machine verification close.

### Future third-party domain packs

**Threats.** Untrusted pack code executing with process privileges; native ABI exploitation; excessive CPU/memory; ambient filesystem/network access; misleading signatures or publisher identity; schema bombs; and a pack bypassing validation, routing, or independent verification.

**Foundation controls.** [ADR-009](adr/009-domain-pack-loading.md) compiles official MVP packs in and rejects native dynamic third-party plugins. The approved future direction is a sandboxed WASM/component boundary; [domain-pack guidance](domain-packs/README.md) denies backend, persistence, credential, network, and Tauri access.

**Implementation gate (post-MVP owning ADR/phase).** Define and version the component interface; validate signed manifests against a real governance identity; enforce memory, fuel, time, nesting, and output limits; expose explicit least-privilege host calls with no ambient filesystem/network; isolate crashes/traps; validate pack schema/migrations and all returned data; preserve typed command transactions and independent verification; provide revocation/update policy; and pass malicious-component tests. Until then, no third-party pack execution path or marketplace is present.

## Deferred controls and closure criteria

| Deferred control | Owner/gate | Closure criterion before enablement |
|---|---|---|
| Strict document/bundle parser, migration, extraction, and atomic import/export | Phase 01 and owning import phases | Adversarial path/archive/limit/checksum/migration fixtures pass with no partial mutation |
| SQLite authority, recovery, backup, journal, revision, and deletion behavior | Phase 01 | Transaction/crash/concurrency/migration/corruption tests pass; secrets are absent from database and bundles |
| Solver-neutral IR capability enforcement and projection | Phase 02 | Versioned deterministic contracts and malformed/unsupported fixtures pass |
| OR-Tools executable, manifest, process supervision, and bounded worker protocol | Phase 03 | Exact source/protobuf gate, target builds, protocol adversarial tests, cancellation, and bundle resolution pass |
| Independent verifier and correctness-alarm quarantine | Phase 04 and each pack phase | Mutation-sensitive differential tests reject a deliberately invalid candidate and authoritative score is recomputed |
| Native credential entry/store and provider authentication | Phases 01 and 10 | OS-store lifecycle and canary leakage tests pass; official OAuth suitability is evidenced if used |
| Provider network/tool boundary | Phase 10 | Injection, redirect/endpoint, malformed stream, bounds, cancellation, stale revision, and redaction tests pass |
| Signing/notarization custody and release identity | Phase 11 | Named governance decision selects real protected custody; build/sign digest handoff and target signing verification pass |
| Updater metadata, channel, rollback, and key rotation | Phase 11 | Signed metadata and tamper/downgrade/wrong-target/expiry/recovery tests pass |
| Sanitized support bundles | Owning diagnostics/release phase | Explicit preview, allowlisted manifest, bounded retention, and seeded-secret final-archive scans pass |
| Sandboxed third-party pack host | Post-MVP numbered ADR and owning phase | Versioned WASM/component ABI, real signing governance, resource limits, no ambient access, and malicious-pack tests pass |

## Review triggers

Stop implementation and write or supersede a numbered ADR before proceeding if a rule cannot be independently verified; a backend requires domain-specific knowledge; the UI requires direct database or credential access; AI requires arbitrary files, code, shell, or solver authority; a plugin requires native in-process loading; a parser or protocol cannot be bounded; migration cannot preserve supported scenarios; packaging requires disabling a major security control; a dependency falls outside license policy; or correctness relies on undocumented provider/backend behavior.

Update this threat model in the same change that adds a trust boundary, network destination, persisted sensitive field, Tauri capability, public parser, worker frame, credential flow, updater/signing path, support-bundle field, or third-party execution mechanism. Documentation alone never closes an implementation gate.
