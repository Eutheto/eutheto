<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-018: Public Scenario Document and Bundle

- **Status:** Approved; format implementation begins in Phase 01
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)

## Context

Backend-native models omit domain meaning, couple users to one solver, and cannot safely serve as the durable interchange contract for migrations, verification, alternate backends, or future packs. The public representation must preserve normalized user intent rather than a compiled optimization artifact.

## Binding decision

> The public external representation is eutheto's versioned scenario document/bundle, never an OR-Tools or MiniZinc model.

The document and portable bundle use project-owned versioned media/schema namespaces. They carry domain meaning and explicit compatibility metadata; planning IR, backend protos, solver models, frontend state, credentials, logs, and unrelated files are not authoritative public scenario content.

## Consequences

- Imports parse, bound, validate, migrate, and checksum untrusted content before one atomic commit.
- Exports are canonical and deterministic, preserve unknown extension data where the contract requires it, and fail safely on unknown newer versions.
- Public fields and tags are never reused with changed meaning; migrations preserve every supported format.
- The final public file extension remains an unresolved product gate and must not be fabricated or registered in Phase 00.
- Backend changes do not require users to rewrite their scenario into a solver format.

## Rejected alternatives

- OR-Tools models/protobufs as the public or persisted project format are rejected.
- MiniZinc models as the public representation are rejected.
- Persisting frontend state or backend planning artifacts as authoritative scenario meaning is rejected.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
