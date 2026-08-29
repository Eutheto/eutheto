<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-003: License and Contribution Attestation

- **Status:** Approved
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)
- **Applies from:** Phase 00

## Context

The project needs a legally clear baseline for source, examples, documentation, official packs and adapters, and project-owned worker code. Contribution attestation must be proportionate for an open-source project and must not imply an organization, contact, or agreement that has not been selected.

## Binding decision

> Project code is Apache-2.0 and contributions initially use DCO sign-off, not a CLA.

Apache-2.0 applies to core, CLI, desktop, official packs/adapters, project worker code, examples, and documentation unless a file or asset is explicitly marked otherwise. Source files use SPDX identifiers where the file type permits comments.

## Consequences

- Contributions require Developer Certificate of Origin sign-off.
- No Contributor License Agreement is required initially.
- Third-party code, data, fonts, assets, models, and solver artifacts retain their own reviewed licenses and notices; Apache-2.0 does not relabel them.
- Official dependency and artifact selection remains subject to the repository allow/review/block license policy.

## Rejected alternatives

- A CLA at project inception is rejected in favor of DCO sign-off.
- Treating a process boundary as automatic permission to ship incompatible solver code is rejected.
- GPL, AGPL, SSPL/source-available, noncommercial/no-derivatives, and proprietary solver binaries are blocked from official artifacts unless policy changes after legal review.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
