<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-001: Reusable Rust Core and Headless CLI

- **Status:** Approved
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)
- **Applies from:** Phase 00 as an architectural boundary; implementation is phased

## Context

The optimization engine must be usable and testable without a desktop runtime. A native desktop shell is one client of the application, not the owner of optimization or authoritative scenario state. This keeps command-line, desktop, and future clients on one application contract and prevents UI concerns from entering domain or solver code.

## Binding decision

> The optimization core is a reusable Rust library with a headless CLI; Tauri is a client.

The core cannot depend on CLI parsing, windows, Tauri types, ambient platform paths, or UI assumptions. The CLI and Tauri adapter call the same typed application services.

## Consequences

- Core and CLI builds and tests can run independently of Tauri.
- Rust owns validation, persistence, routing, solving, verification, scoring, explanations, and import/export.
- Tauri commands remain a thin, coarse-grained client boundary.
- Desktop-only convenience cannot become mock or competing domain authority.

## Rejected alternatives

- Putting optimization or authoritative scenario state in Tauri or Vue is rejected because it creates client-specific authority.
- Coupling the reusable core to desktop lifecycle or CLI parsing is rejected because it prevents headless reuse.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
