<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-012: Tauri API and Generated TypeScript DTOs

- **Status:** Approved
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)
- **Contributor rules:** [Generated code and contracts](../contributors/generated-code-and-contracts.md)

## Context

A large fine-grained desktop IPC surface increases permission, compatibility, and state-consistency risk. Hand-maintained Rust/TypeScript DTO pairs drift. The UI needs typed application operations and view data, not direct database or solver access.

## Binding decision

> Tauri exposes coarse-grained commands and generated checked-in TypeScript DTOs; Rust owns authoritative scenario state.

Only the desktop API layer may invoke Tauri commands or subscribe to Tauri events. Custom commands are registered and capability-scoped through Tauri's application manifest; an invoke handler alone is not treated as authorization.

## Consequences

- Rust contract sources generate TypeScript DTOs through the repository generation command.
- Generated files are checked in when release builds consume them or onboarding materially benefits, are never hand-edited, and must pass clean-tree drift checks.
- Pinia/query state is a presentation cache and cannot become authoritative.
- Command/event payloads are versioned, bounded, least-privilege, and avoid raw credentials or unrestricted paths.
- Phase 00's shell API does not imply future domain commands are implemented.

## Rejected alternatives

- Hand-maintained duplicate DTO definitions are rejected because they drift.
- Direct `invoke`/event imports throughout Vue components are rejected.
- Direct database access or authoritative scenario mutation in the webview is rejected.
- Treating `invoke_handler` registration alone as an adequate capability boundary is rejected.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
