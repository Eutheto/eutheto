<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-010: Local State and Credential Storage

- **Status:** Approved
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)

## Context

A local-first application needs a durable authority for projects while keeping provider secrets out of ordinary application data, exports, frontend state, and diagnostics. Credential lifetime and access controls differ from scenario persistence and belong to the operating system's protected facility.

## Binding decision

> SQLite is the local source of truth; credential values are stored only in the OS credential store.

SQLite is accessed behind a dedicated Rust service. Vue receives only opaque credential references and status; credential values never enter Vue/JavaScript state, SQLite, bundles, logs, Nix derivations, repository files, or ordinary IPC responses.

## Consequences

- Authoritative scenarios, revision state, journal, and related local metadata live in transactional SQLite rather than frontend stores.
- Credential APIs accept secrets only at a Rust/native-owned secure boundary, store through the OS credential service, and return no raw secret.
- Portable exports exclude credentials, tokens, logs, and unrelated paths.
- Database backup, migration, corruption recovery, concurrency, permissions, and deletion behavior require explicit later-phase implementation and tests.

## Rejected alternatives

- Storing API keys, OAuth tokens, or other credential values in SQLite is rejected.
- Putting secrets in Vue/Pinia, `.env` files, ordinary IPC payloads, logs, caches, or Nix derivations is rejected.
- Treating frontend state as scenario authority is rejected.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
