<!-- SPDX-License-Identifier: Apache-2.0 -->

# Compatibility Policy

## Scope and phase boundary

This policy applies once introduced to every public scenario document, portable bundle, database schema, application command/event/DTO, CLI machine-readable output, domain-pack document, persisted or exchanged planning/projection record, worker protocol, release manifest, and updater metadata format. [ADR-018](../adr/018-public-scenario-representation.md) makes the versioned eutheto scenario document/bundle—not a backend model—the public interchange authority. [ADR-012](../adr/012-tauri-api-and-generated-dtos.md) governs generated desktop DTOs, and [ADR-004](../adr/004-ortools-worker.md) governs the isolated worker protocol.

[Phase 00](../roadmap/00-repository-and-reproducible-tooling.md) establishes these rules and generation scaffolding only. It does not claim a released schema or a working solver peer. Phase 01 implements scenario, bundle, database, command, event, and DTO compatibility; Phase 02 introduces domain/planning contracts; Phase 03 implements and proves the worker protocol with an approved matched solver/protobuf toolchain; Phase 11 assembles release matrices; and Phase 12 requires every-version migration and protocol evidence before publication.

## Version contract

Every public surface must have:

- one authoritative version declaration, not a version inferred from application code or package metadata;
- documented current writer version and exact reader support window;
- a compatibility classification for each change;
- typed `malformed`, `unsupported-version`, `incompatible`, and `resource-limit` failures as applicable;
- centralized byte, item-count, nesting, string, frame, and decompression limits; and
- canonical serialization rules where bytes are hashed, signed, compared, or used as fixtures.

Writers emit only the current version. Readers support exactly the documented window—never an accidental range accepted by a permissive parser. An application release manifest records independently versioned application, core API, scenario envelope, domain schema, planning IR, worker protocol, solver, adapter, and applicable policy/catalog versions together without pretending those versions are interchangeable.

## Change classification

A change is **compatible** only when all supported readers retain the documented meaning and safety properties. Additive fields are not automatically compatible: their absence/default semantics, unknown-field behavior, canonical form, bounds, and old-reader behavior must be specified and tested.

A change is **migration-requiring** when supported older data can be deterministically transformed to the current form without losing promised meaning. Migration logic lands before current writers depend on the new form. Imports parse and validate into staging, complete all required migrations, and commit once atomically.

A change is **breaking** when it changes existing meaning, removes required supported meaning, violates the documented reader window, or cannot preserve supported data. It requires the format's declared incompatible-version transition, an overall major release where the public CLI/API/document contract is incompatible, updated support policy, migrations or explicit refusal, and release notes. A clean internal cutover does not excuse breaking a released external contract.

## Unknown-newer behavior

An unknown newer outer envelope, bundle, database, domain-pack, persisted-record, updater-metadata, or protocol version is rejected safely before mutation, migration, worker execution, update installation, or other authoritative side effect. The error must identify the unsupported surface and version without exposing untrusted payload details. There is no best-effort downgrade, silent default to the current version, or partial import.

Unknown data inside an otherwise supported version follows that format's explicit rule:

- designated extension data is preserved losslessly through read/write and migration when forward-compatible preservation is part of the contract;
- unknown enum or tagged-union values are never coerced to a known default;
- non-extension unknown fields may be rejected when the schema is closed; and
- ignored protocol fields or messages are permitted only when the negotiated protocol version explicitly defines that behavior and no safety, lifecycle, or authority decision depends on them.

Preserving opaque extension data does not make it trusted or executable. It remains bounded and cannot influence validation, routing, solving, credentials, filesystem access, or UI rendering without a recognized schema.

## Permanent field and tag identity

Once released, a field name, numeric field tag, enum numeric value, tagged-union discriminator, command/event name, stable error code, or protocol message kind permanently retains its meaning. Removal retires the identifier; it never makes the identifier available for a different meaning. Protobuf field numbers and enum values must be declared reserved when removed, and generated bindings must retain that reservation.

Renaming while reusing the same identifier for changed semantics is a breaking change, not a migration shortcut. A new meaning receives a new identifier and versioned migration. Aliases, dual writers, deprecated duplicate fields, and silent fallbacks are forbidden unless the published compatibility contract specifically requires a bounded read path. Internal callers migrate in the same change and obsolete write paths are removed.

## Worker protocol compatibility

The authoritative protocol source, protocol version declaration, generator and runtime, matched upstream protobuf inputs, generated bindings, worker, desktop adapter, golden frames, hashes, and manifests are one reviewed contract. Peers negotiate before accepting model/request traffic. An unsupported version, capability, target, source/hash, or lifecycle contract produces a typed incompatibility and starts no solve.

A protocol change must define request and response limits, request-ID matching, lifecycle and terminal states, cancellation, unknown-message behavior, and failure recovery. Malformed, truncated, oversized, duplicate, stale, out-of-order, or unknown worker output cannot mutate scenario state or crash the desktop process. The exact OR-Tools/protobuf pin, source hash, and build flags remain a Phase-03 gate; this Phase-00 policy supplies none of those values.

## Required migration and compatibility evidence

A public schema or protocol change is complete only when the authoritative input, generated products, every reader/writer/caller, compatibility declaration, documentation, and focused evidence agree. The evidence set includes, as applicable:

1. immutable fixtures for every supported released version and the expected canonical current form;
2. sequential migration from each older version plus every direct path the support policy promises;
3. canonical read/write round trips, deterministic ordering and hash stability;
4. lossless round trips and migrations for designated unknown extension data;
5. safe unknown-newer refusal and downgrade guards;
6. malformed, duplicate-key/tag, unknown-enum/union, size/count/nesting/decompression, overflow, and checksum cases;
7. interrupted migration rollback, backup/restore, restart, export-after-migration, and one-atomic-commit behavior;
8. old/new protocol handshake and capability matrices, golden frames, truncation, oversized/malformed frames, unknown tags/messages, request-ID mismatch, duplicate/out-of-order lifecycle messages, early exit, timeout, and cancellation;
9. proof that incompatibility or malformed input cannot partially mutate authoritative state; and
10. deterministic generation and clean-tree generated-artifact drift checks.

Released migration files and compatibility fixtures are immutable evidence. A new migration is additive and carries before/after fixtures; prior files are not rewritten to make history appear current. [Generated artifact policy](generated-artifacts.md) governs regeneration, ownership, source hashes, and checked-in outputs. Phase 12 accepts compatibility claims only when this evidence is bound to the identical release-candidate digests.
