<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-006: Solver-Neutral Planning IR

- **Status:** Approved
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)
- **Detailed boundary:** [Domain-pack guidance](../domain-packs/README.md)

## Context

Domain models express human meaning; solver APIs express backend-specific mechanics. Collapsing those layers would couple official packs to a solver, prevent capability-based routing, and make independent verification and alternative backends harder to reason about.

## Binding decision

> Domain packs compile to solver-neutral planning IR and never construct backend objects.

Domain model/domain IR remains pack-specific normalized meaning. Planning IR is shared, immutable, typed Boolean/integer/interval mathematics. A backend model is adapter-private.

## Consequences

- Domain packs depend only on the domain API and planning IR, never OR-Tools, Pumpkin, or backend adapters.
- Backend adapters consume validated planning IR and never depend on official domain packs.
- Planning IR requires explicit versions, semantics, capabilities, provenance, projection, deterministic ordering, and checked integer arithmetic.
- Unsupported primitives are rejected by capability checking before execution rather than leaking backend details into packs.

## Rejected alternatives

- Constructing CP-SAT, Pumpkin, MiniZinc, or other backend objects in domain packs is rejected.
- A universal untrusted string expression language is rejected in favor of typed IR.
- Collapsing domain IR, planning IR, and backend models into one representation is rejected.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
