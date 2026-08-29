<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-011: Command Journal, Snapshots, and Undo/Redo

- **Status:** Approved
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)

## Context

Desktop, imports, CLI, and later AI need one mutation path with validation, revision conflict detection, atomicity, durability, auditability, and reversible user interactions. A local application does not need distributed event-sourcing infrastructure to provide history.

## Binding decision

> Scenario mutation uses typed commands, batches, inverses, periodic snapshots, and undo/redo—not distributed event sourcing.

Every completed mutation is validated and applied in one revision-checked SQLite transaction. Reversible commands record inverses; batch inverses reverse member order and apply atomically.

## Consequences

- All mutation sources use the same command service and `expected_revision` discipline.
- Failed commands and batches leave no partial authoritative change.
- Periodic snapshots bound replay while the journal supports undo/redo and audit behavior.
- A new command after undo truncates or explicitly branches redo history according to the application contract.
- Semantically irreversible actions must be explicit rather than receiving a fabricated inverse.

## Rejected alternatives

- Distributed event sourcing is rejected as unnecessary complexity for the local-first mutation contract.
- Direct table writes, frontend mutations, and import-specific bypasses are rejected.
- Best-effort multi-step mutation outside one transaction is rejected.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
