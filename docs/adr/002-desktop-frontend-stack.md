<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-002: Desktop Frontend Stack

- **Status:** Approved
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)
- **Applies from:** Phase 00 for the minimal desktop boundary

## Context

The repository needs one deliberately small desktop web stack while keeping the future documentation/site application separate. The desktop client must remain a presentation layer over generated application DTOs and the thin Tauri adapter.

## Binding decision

> Desktop uses Vue 3, TypeScript, and Vite; Nuxt is reserved for the future docs/site app.

Nuxt is not part of the desktop runtime. The `apps/docs` location is reserved for post-MVP work and does not imply a Phase-00 implementation.

## Consequences

- Desktop code follows strict TypeScript and Vue conventions and is built with Vite.
- Desktop dependencies must not introduce Nuxt server/runtime assumptions.
- The future site may choose Nuxt within its reserved boundary without changing the desktop stack.
- Phase 00 provides only a minimal real shell, not a design system or domain feature.

## Rejected alternatives

- Nuxt as the desktop application layer is rejected and reserved for the future docs/site app.
- Electron dependencies are excluded from the initial architecture; Tauri is the selected desktop client boundary.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
