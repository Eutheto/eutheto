<!-- SPDX-License-Identifier: Apache-2.0 -->

# Repository Layout

The [Phase-00 roadmap](../roadmap/00-repository-and-reproducible-tooling.md#repository-layout) defines stable responsibility boundaries so later phases do not grow a second convention beside the intended architecture. A directory in that target shape is not, by itself, an implemented module, workspace member, public package, or compatibility promise.

## Workspace authority and the inventory gate

`Cargo.toml` is the sole authority for Rust workspace membership. Phase 00 has exactly five real Rust members:

- `crates/eutheto-types`;
- `crates/eutheto-core`;
- `crates/eutheto-cli`;
- `apps/desktop/src-tauri`; and
- `xtask`.

The other roadmap names under `crates/` are retained by boundary READMEs only. They have no `Cargo.toml`, code, or public API. A later owning phase must decide whether each responsibility still warrants a separate crate before adding it to the workspace. Renaming, combining, or declining a reserved crate requires the same explicit exact-inventory architecture gate; a README reservation does not pre-approve a crate.

Likewise, `pnpm-workspace.yaml` supplies directory patterns, but a directory is not a JavaScript workspace project without a real `package.json`. The reservations under `packages/` and `apps/docs/` are not installed or publishable packages. Their eventual inventory, visibility, dependency graph, and release status remain gated decisions. In particular, an npm-style heading such as `@eutheto/ui` records the project namespace and intended boundary, not a publication commitment.

This distinction keeps the complete roadmap shape visible without freezing speculative manifests or making absent behavior appear implemented.

## Dependency direction

The normative direction is:

```text
presentation → thin Tauri adapter → application services → domain packs
             → planning core → backend adapters → infrastructure
```

An arrow permits narrowly defined knowledge; it does not grant unrestricted imports of everything to the right. Interfaces are owned at the inward boundary, and infrastructure implements application-owned ports. The detailed rules remain authoritative in [Dependency Boundaries](dependency-boundaries.md).

Applied to this layout:

- `apps/desktop` and future domain `ui/` directories are presentation or adapter surfaces. They use generated application contracts and never own scenario validation, persistence, feasibility, or scoring.
- `packages/frontend-api` is reserved for the generated DTO/transport boundary; Tauri invoke and event primitives stay inside the desktop API facade.
- `packages/frontend-core` and `packages/ui` remain presentation-only. Frontend stores and query caches are not authoritative state.
- `domains/*/core` may eventually depend on the domain API, stable value types, solver-neutral planning IR, and normalized verification contracts. Official packs do not depend on Tauri, SQLite, credentials, network providers, OR-Tools, Pumpkin, or backend-native objects.
- Planning IR and solver interfaces must not depend on official domain-pack implementations.
- Backend adapters consume validated solver-neutral IR and produce untrusted candidates. They do not mutate persistence or decide authoritative domain feasibility and scores.
- Infrastructure implements application-owned persistence, process, filesystem, network, clock, and credential ports without acquiring domain policy.
- Optional AI receives bounded typed context and may propose typed commands only; it has no persistence, credential, shell/code, solver, verifier, routing, or mutation authority.

## Reserved Rust boundaries

The real Phase-00 members `eutheto-types`, `eutheto-core`, and `eutheto-cli` are implemented by their owning foundation slices. The following names are only future responsibility reservations:

| Reserved path | Intended boundary; not current behavior |
|---|---|
| `crates/eutheto-domain-api` | Domain-pack interfaces and registration contracts. |
| `crates/eutheto-domain-ir` | Normalized domain representation shared across pack and verification boundaries. |
| `crates/eutheto-planning-ir` | Solver-neutral primitives, capabilities, provenance, and projection contracts. |
| `crates/eutheto-protocol` | Bounded, versioned Rust ownership for the solver-worker protocol and generated types. |
| `crates/eutheto-store` | Persistence infrastructure implementing application-owned ports. |
| `crates/eutheto-command` | Typed application commands, queries, journal, and undo services. |
| `crates/eutheto-solver-api` | Backend-neutral solver request, capability, candidate, cancellation, and error interfaces. |
| `crates/eutheto-solver-router` | Capability routing and decomposition policy. |
| `crates/eutheto-solver-ortools` | Adapter to an isolated OR-Tools worker; OR-Tools 9.15 remains an unapproved candidate until Phase 03 closes its source, hash, protobuf, build, license, callback, assumption-core, and benchmark gates. |
| `crates/eutheto-solver-pumpkin` | Experimental future Pumpkin adapter, deferred to its dedicated phase and gates. |
| `crates/eutheto-verify` | Projection, independent rule evaluation, and authoritative score recomputation. |
| `crates/eutheto-explain` | Explanations derived from verified domain facts and provenance. |
| `crates/eutheto-ai` | Optional bounded AI proposal/query adapter. |
| `crates/eutheto-import` | Bounded parsing, migration, validation, and atomic import boundary. |
| `crates/eutheto-export` | Canonical, versioned scenario and bundle export boundary. |

None of these reservations authorizes a no-op executable, dummy worker, backend placeholder, provider stub, or fabricated generated source. The owning phase must add a complete tested contract or leave the boundary reserved.

## Official domain-pack shape

Each official domain reserves the same four internal responsibilities:

```text
domains/
├── workforce/{core,ui,fixtures,docs}
├── seating/{core,ui,fixtures,docs}
└── school/{core,ui,fixtures,docs}
```

`core/` is the future Rust pack boundary; `ui/` is presentation only; `fixtures/` will hold synthetic or sanitized deterministic contract inputs; and `docs/` will describe behavior only after it exists. These directories contain no domain manifests or behavior in Phase 00 and do not settle whether each responsibility becomes a separate crate or package.

## Shared frontend and documentation reservations

```text
packages/
├── ui
├── frontend-api
├── frontend-core
└── test-fixtures

apps/docs/                 # post-MVP Nuxt reservation
```

The four package paths reserve shared presentation, generated API, platform-neutral frontend, and test-input responsibilities respectively. Production code must not depend on `test-fixtures`. The Nuxt docs application is post-MVP and currently has no manifest, framework configuration, hosting identity, or deployment contract. Authoritative project documentation remains under `docs/` until that later application is approved and implemented.

## Adding a member later

An owning roadmap phase may turn a reservation into a real member only when it supplies all of the following together:

1. an approved responsibility and dependency position consistent with the architecture rules;
2. a real manifest with non-floating dependencies and an explicit private/public decision;
3. complete implementation for that phase rather than a placeholder or successful no-op;
4. contract tests and architecture enforcement appropriate to the new boundary; and
5. updates to workspace membership, generated-code ownership, documentation, license inputs, and CI in the same change.

Until then, the substantive README is the entire contract: the path and responsibility are reserved, while implementation, API stability, and the exact crate/package inventory remain open.
