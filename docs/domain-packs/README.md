<!-- SPDX-License-Identifier: Apache-2.0 -->

# Domain-Pack Boundary

A domain pack owns normalized human meaning for one planning domain. It does not own persistence, UI transport, credentials, backend selection, or a solver. This document records the boundary established by [ADR-006](../adr/006-solver-neutral-planning-ir.md) and [ADR-009](../adr/009-domain-pack-loading.md). The executable interfaces arrive in [Phase 02](../roadmap/02-domain-pack-and-planning-ir-contracts.md); Phase 00 provides guidance only and does not implement workforce, seating, or school behavior.

## Responsibilities

An official pack may provide, through stable pack/domain interfaces:

- a versioned descriptor, schema/document version, migrations, and normalized domain model;
- typed commands and fast/full domain validation;
- deterministic normalization and compilation to solver-neutral planning IR;
- complete provenance and projection between planning variables and stable domain assignments;
- independent verification against the original normalized domain scenario;
- verifier-owned authoritative score interpretation;
- typed, deterministic explanation evidence and message parameters;
- import/export adapters for reviewed domain formats; and
- pack-specific metadata and UI contributions through versioned data contracts.

A pack implementation is not complete when it merely emits a solver constraint. A rule is complete only with document/command representation, validation, deterministic planning-IR compilation, capability declaration, projection/provenance, independent verifier evaluation, authoritative scoring when applicable, explanation evidence, compatibility fixtures, edge/infeasible cases, and applicable import/export/UI documentation.

## Forbidden dependencies and capabilities

A domain pack must not:

- construct or invoke OR-Tools, Pumpkin, MiniZinc, or any backend object;
- choose a backend or bypass capability-based deterministic routing;
- access SQLite, database tables, OS credentials, provider clients, Tauri, or Vue;
- read ambient filesystem paths, environment, locale, time zone, wall clock, core count, or network during migrate/validate/compile/verify;
- mutate authoritative state outside the application command transaction;
- return pre-rendered untrusted HTML or execute user expressions; or
- treat compiler constraints, backend status, or backend objective values as verifier authority.

Backends reciprocally must not depend on official pack crates. They consume validated planning IR and return typed assignments and bounded evidence, not trusted domain solutions.

## Data model and deterministic compilation

Domain model/domain IR is pack-specific normalized human meaning. Planning IR is shared immutable Boolean/integer/interval mathematics. Backend models are adapter-private. Crossing each boundary requires an explicit typed conversion.

For the same scenario revision and explicit compile context/options, a pack produces byte-equivalent canonical planning IR, provenance, and projection regardless of input map order, workstation locale, host time zone, current clock, machine core count, or hash-map iteration. Stable typed IDs—not names, positions, or database row IDs—identify all entities. Scaling, bounds, products, sums, and score aggregation use checked integer arithmetic and fail before routing on overflow.

Required rules compile as mandatory constraints. Preferences are explicit bounded objective expressions and cannot silently relax requirements. Every planning primitive and semantic variant declares capabilities so an incompatible backend is rejected before solving.

## Projection, verification, and explanations

Projection maps validated backend values to a versioned normalized solution with stable domain assignment IDs. It rejects missing required values, unknown variables, invalid value types, and inconsistent assignments. The projected result still is not accepted.

The pack's verifier evaluates the original normalized domain scenario and projected solution independently of backend status and compiler records. Shared primitive facts are acceptable only when their semantics are independently tested. Accepted feasibility is zero violations; score/category breakdown is recomputed by verifier-owned logic. Explanations use typed evidence and exact certainty labels. AI may paraphrase that evidence but cannot replace or alter it. See [ADR-007](../adr/007-independent-solution-verification.md).

## Registration and distribution

Official MVP packs are compiled into and explicitly registered by the application. Their source, fixtures, assets, and dependencies are reviewed under Apache-2.0 and the repository license policy; imported user assets remain user data and are not relicensed.

Future external-data-backed official packs follow the boundary planned for [Phase 14 transportation](../roadmap/14-transportation-domain-pack.md); this is guidance for post-MVP work, not a claim that the pack or any provider integration currently ships. Reviewed Rust application/infrastructure adapters—not the pack—authenticate and fetch from calendar, routing, or transit providers, validate and normalize responses, enforce provider/licensing/caching/privacy terms, and produce bounded immutable local snapshots with explicit provenance and freshness. The pack receives only those snapshots (or equivalent manual local input) through versioned provider-neutral contracts. It never sees provider clients or identities, credentials/tokens, network requests, or ambient network capability, and its migrate/validate/compile/verify paths remain deterministic and network-free. Snapshot persistence and export are application policies gated by the provider's terms and user privacy controls, not pack-owned I/O.

Third-party packs are future work. Their approved direction is a narrow sandboxed WASM/component host with a versioned interface, signed manifest, bounded memory/fuel, explicit host calls, and no ambient filesystem or network. Native dynamic libraries, an unstable Rust plugin ABI, and in-process third-party native code are prohibited. Do not create a placeholder marketplace, fake loader, no-op host, or production capability before its owning phase closes security, compatibility, signing, and resource gates.

## Public documents and migrations

Pack data lives inside eutheto's versioned scenario document/bundle, never a backend model ([ADR-018](../adr/018-public-scenario-representation.md)). Unknown newer pack versions fail safely. Unknown extension data is preserved when the format contract requires it. Migrations are deterministic, bounded, network-free, and preserve supported scenario meaning before one atomic application commit.

Schema and generated artifacts follow [contributor contract discipline](../contributors/generated-code-and-contracts.md). Generated sources and descriptors are never hand-edited.
