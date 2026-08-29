<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-004: OR-Tools CP-SAT Worker

- **Status:** Approved architecture; backend implementation is gated to Phase 03
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)

## Context

OR-Tools is a native C++ dependency and its output is untrusted candidate data. Keeping it out of the desktop process provides crash isolation, per-solve process-tree cancellation, version negotiation, bounded protocol handling, cleanup, and upgrade safety. The exact OR-Tools/protobuf source pin remains subject to build, license, callback, assumption-core, packaging, and benchmark evidence.

## Binding decision

> OR-Tools CP-SAT is the stable primary backend in a bundled native worker process.

The project-owned worker communicates through a bounded, versioned protocol. It is resolved from the application bundle rather than `PATH`, and one worker child is launched per solve for MVP.

## Consequences

- The desktop/core process supervises rather than links the primary backend in process.
- Worker absence, mismatch, malformed frames, crash, timeout, and cancellation are recoverable typed errors and cannot mutate scenario state.
- OR-Tools and its protobuf definitions are pinned and tested as one contract.
- Worker source/hash, protocol, capability, version, target, linkage, license, and release metadata must be recorded.
- “Stable primary backend” is an architectural role, not a claim that Phase 00 ships a worker; Phase 03 must close the exact pin and artifact gates.

## Rejected alternatives

- In-process OR-Tools in the desktop process is rejected because it weakens crash and cancellation isolation.
- Resolving the bundled worker from ambient `PATH` is rejected.
- Shipping a dummy worker or guessing CMake/protobuf compatibility is rejected.
- A long-lived worker pool is deferred beyond MVP unless it preserves the same isolation properties.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
