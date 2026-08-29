<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-017: Explicit Time Semantics and Integer Solver Units

- **Status:** Approved
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)

## Context

Scheduling semantics change across time zones, daylight-saving gaps and overlaps, locale assumptions, and host clocks. Floating-point or unchecked conversion into solver quantities can introduce nondeterminism, rounding errors, or overflow before a backend sees the model.

## Binding decision

> Time has an explicit scenario IANA zone and DST policy; solver quantities use checked integer units.

A time-based scenario stores its IANA zone, locale, horizon, DST gap/overlap policy, intended local values, resolved instants, and display values as required by its versioned format. Solver durations are whole minutes in MVP unless a later ADR changes the public unit contract.

## Consequences

- The host time zone, locale, wall clock, core count, and ambient environment never silently determine canonical compilation.
- Nonexistent and ambiguous local times are rejected, explicitly resolved, or handled by a documented pack rule; they are never guessed.
- Elapsed-time and local/scheduled-time semantics remain distinct and visible.
- All scaling, bounds, sums, products, and objective aggregation use checked integer arithmetic and reject overflow before backend translation.
- DST boundary fixtures and deterministic fixed clocks are mandatory when the time implementation lands.

## Rejected alternatives

- Inferring a scenario zone from the current host is rejected.
- Silently choosing an offset for DST gaps or overlaps is rejected.
- Floating-point solver quantities and unchecked integer scaling are rejected for canonical planning work.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
