<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-007: Independent Solution Verification

- **Status:** Approved
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)

## Context

A backend status, objective, or assignment can be wrong because of compilation, protocol, projection, backend, or integration defects. Treating solver output as authoritative would make the same translation path both producer and judge. Acceptance therefore needs a separate evaluation of original normalized domain meaning.

## Binding decision

> Every accepted solution is independently verified against the original domain scenario.

A candidate is projected to normalized domain assignments, structurally validated, checked against every required domain rule, and scored authoritatively by verifier-owned logic before it can be exposed or persisted as accepted.

## Consequences

- Backend results remain untrusted candidates; backend feasibility and scalar objectives are not authoritative user results.
- Accepted solutions require zero feasibility violations and authoritative score recomputation.
- Verification uses the normalized domain scenario and normalized solution, not backend status or compiled constraints as rule evidence.
- Failed verification is a correctness alarm: quarantine the candidate, do not mutate the scenario, and preserve bounded diagnostic evidence.
- Explanations and proof wording must match verified facts and actual backend proof state.

## Rejected alternatives

- Trusting a solver's `FEASIBLE` or `OPTIMAL` status as domain-rule evidence is rejected.
- Reusing compiler records as the verifier is rejected where it would repeat the same defect.
- Exposing an unverified incumbent as ready is rejected.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
