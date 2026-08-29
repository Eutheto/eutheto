# AGENTS.md

## Project

`eutheto` is a local-first, open-source constraint-optimization platform. The repository is currently in the planning and bootstrap stage; implementation proceeds through the dependency-gated phases in [`docs/roadmap/`](docs/roadmap/README.md).

Do not claim, document, or test behavior that has not been implemented. Do not create mock authority in the desktop client to make an incomplete vertical slice appear functional.

## Source of truth

Use this authority order when requirements conflict:

1. approved security and data-integrity ADRs;
2. current scenario, bundle, and worker-protocol schemas plus conformance tests;
3. [`docs/roadmap/README.md`](docs/roadmap/README.md) and the active phase document;
4. generated documentation;
5. code comments.

[`docs/roadmap/assumptions.md`](docs/roadmap/assumptions.md) records dated package and tooling evidence. Reverify mutable external contracts before adopting or updating them. Do not silently replace a roadmap requirement with an assumption.

## Delivery discipline

- Work only within the current phase and its completed prerequisites. A later phase may influence a contract fixture, but its production feature does not move earlier.
- Preserve issue IDs, acceptance criteria, exit gates, non-goals, and compatibility requirements from the applicable phase document.
- Build complete vertical behavior rather than disconnected scaffolds or placeholders.
- Make the smallest coherent change. Do not add speculative abstractions, dependencies, configuration, fallbacks, or feature flags.
- Update every affected caller, test, schema, generated artifact, and document in the same change. Use clean cutovers; do not leave deprecated aliases unless a published compatibility contract requires them.
- Never hand-edit generated files. Change their authoritative inputs and regenerate them through the repository command.

## Architecture boundaries

Dependency direction is presentation → thin Tauri adapter → application services → domain packs → planning core → backend adapters/infrastructure.

- Rust owns authoritative scenario state, validation, persistence, routing, solving, verification, scoring, explanations, and import/export.
- Tauri is a client boundary, not the optimizer. Only the desktop API layer may invoke Tauri commands or subscribe to Tauri events.
- Domain packs depend on the domain API and solver-neutral planning IR. They never construct backend objects or depend on Tauri, SQLite, credentials, network providers, OR-Tools, or Pumpkin.
- Solver adapters depend on planning IR and solver APIs, never official domain packs.
- AI may read bounded context and propose typed application commands. It cannot bypass validation, mutate persistence directly, access arbitrary files, execute shell/code, or act as solver/verifier authority.
- Every accepted candidate is projected and independently verified against the original domain scenario, with authoritative score recomputation.
- Hidden global mutable state is forbidden.
- `unsafe` is forbidden in normal crates. A narrow exception requires an isolated safety module, documented invariants, dedicated tests, and maintainer review.

## Data and protocol rules

- Mutations are typed, validated, transactional, revision-checked, durable, and reversible when semantics permit.
- Solves operate on immutable scenario revisions. Stale results remain explicit and cannot be silently applied.
- Stable typed IDs identify entities; never use display names, collection positions, or database row IDs as identity.
- Public scenario, bundle, database, command, event, and worker-protocol formats are versioned. Never reuse a field or tag with different meaning.
- Unknown newer versions fail safely. Preserve unknown extension data where the format contract requires forward compatibility.
- Canonical serialization and hashing use stable ordering and checked arithmetic.
- Imported files, bundles, worker frames, provider responses, and URLs are untrusted, bounded inputs. Parse and validate before one atomic commit.
- OR-Tools communicates through the versioned worker protocol and remains isolated from the desktop process. Worker failure must not corrupt scenario state.

## Security and privacy

- Secrets never enter Vue/JavaScript state, logs, SQLite, exports, diagnostics, Nix derivations, repository files, or normal IPC payloads.
- Credentials are entered through a Rust/native-owned secure surface and stored only in the operating-system credential store; Vue receives opaque references and status.
- Use least-privilege Tauri commands, capabilities, CSP, filesystem grants, CI permissions, and release jobs.
- Do not add telemetry or network access by default. AI and provider integrations remain optional and explicit.
- Never commit `.env` files, credentials, signing material, local databases, captured user scenarios, or unsanitized support bundles.
- Treat dependency licenses, install scripts, advisories, SBOM inputs, updater metadata, and signing custody as release constraints.

## User interface and accessibility

- Use human domain language before solver terminology: `Required`, `Preference`, `Optimize`, and `Repair plan`.
- Rust remains authoritative; Pinia and query state are presentation caches only.
- Every primary flow is keyboard-complete, has correct focus behavior and screen-reader names/announcements, and does not rely on color alone.
- Canvas and charts require an equivalent accessible list/table representation.
- Implement normal, empty, loading, stale, error, cancellation, and offline-capable states as applicable.

## Commands and generated artifacts

Once Phase 00 creates them, `Justfile` is the human command authority and `xtask` owns cross-platform generation, hashing, solver assembly, licenses, SBOMs, fixtures, and release manifests. Prefer those commands over ad hoc scripts.

Expected command families include formatting checks, linting, type checking, Rust and JavaScript tests, protocol/generation drift checks, worker build/smoke, migration tests, desktop E2E, license policy, SBOM generation, benchmarks, Nix checks, and release preflight. Until a command exists, do not invent a successful invocation in documentation.

Commit `Cargo.lock`, `pnpm-lock.yaml`, `flake.lock`, exact CI action SHAs, worker source/hash metadata, and required generated protocol/DTO/schema artifacts. Build output, local caches, test reports, local databases, and secrets remain ignored.

## Verification

- Bugs: reproduce the failure, fix its causal mechanism, and rerun the reproducer.
- Features and contract changes: exercise the observable path and run the narrowest existing checks that prove it.
- Schema/protocol/migration changes: run compatibility, round-trip, unknown-version, generated-drift, and malformed-input fixtures.
- Solver changes: run capability, translation, cancellation, worker-failure, projection, independent-verifier, score, deterministic-fixture, and applicable benchmark checks.
- Desktop changes: run focused type/lint/component checks and exercise the packaged Tauri surface where the changed behavior depends on native integration.
- Security-sensitive changes: test both request and response boundaries and confirm diagnostics/support artifacts remain redacted.
- Report only checks actually run. A unit test does not prove packaging, and a successful build does not prove product acceptance.

## Repository identity

Rust crates use `eutheto-*`; npm packages use `@eutheto/*`; project-owned media types use the `eutheto/...` namespace. The CLI executable, reverse-domain application IDs, portable project extension, hosting organization, governance/security contacts, and signing identities remain explicit roadmap gates. Do not publish placeholders as final identifiers.
