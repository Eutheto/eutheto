<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-005: Experimental Pumpkin Backend

- **Status:** Approved architecture; implementation deferred to Phase 08 gates
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)

## Context

A Rust-native backend may offer useful capabilities without a worker boundary, but in-process execution has different ownership, panic, cancellation, and resource-isolation risks. Backend labels must reflect demonstrated compatibility and performance rather than language preference.

## Binding decision

> Pumpkin is an in-process Rust backend labelled experimental until compatibility, cancellation, packaging, and benchmark gates pass.

Pumpkin is never selected automatically in the public MVP unless experimental backends are explicitly enabled and its generated support matrix has no gap for the model.

## Consequences

- Phase 00 does not add a Pumpkin implementation or claim support.
- Phase 08 must establish its exact supported primitive matrix, dedicated-thread ownership, cooperative cancellation, panic containment, packaging, licensing, and benchmark evidence.
- Every Pumpkin candidate remains subject to the same projection and independent verification boundary as every other backend.
- Its experimental label cannot be removed by documentation alone.

## Rejected alternatives

- Presenting Pumpkin as stable before the named gates pass is rejected.
- Automatically routing public-MVP work to an experimental backend is rejected.
- Treating Rust-native execution as sufficient evidence of cancellation, compatibility, or correctness is rejected.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
