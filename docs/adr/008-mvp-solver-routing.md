<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-008: MVP Solver Routing and Decomposition

- **Status:** Approved
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)

## Context

Splitting a model across backends is correct only when no constraint, objective, score normalization, projection rule, or domain invariant crosses the split. Apparent separation by time, location, or entity group is insufficient; fairness commonly creates global coupling. MVP routing must prioritize explainable correctness over portfolio sophistication.

## Binding decision

> MVP routing chooses one backend for a connected model and splits only mathematically and semantically independent components; cross-solver decomposition is post-MVP.

Routing is deterministic code. Component independence requires both planning-hypergraph proof and domain-semantic proof, including safe merge of projected values.

## Consequences

- A compatible explicit override may be honored; otherwise routing follows the documented capability policy.
- Routing reasons, compatibility, and model fingerprint are recorded and explainable.
- A split is forbidden if any constraint, multi-variable objective, global score normalization, projection, or domain invariant spans components.
- Concurrent portfolio execution and cross-solver decomposition are not MVP capabilities.

## Rejected alternatives

- Splitting merely because assignments differ by time or location is rejected.
- AI-selected routing is rejected; routing is deterministic application policy.
- Cross-solver decomposition and concurrent portfolio solving are deferred beyond MVP.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
