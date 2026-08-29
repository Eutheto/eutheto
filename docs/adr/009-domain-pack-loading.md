<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-009: Official and Third-Party Domain Pack Loading

- **Status:** Approved; third-party host is future work
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)
- **Detailed boundary:** [Domain-pack guidance](../domain-packs/README.md)

## Context

Official packs need stable Rust integration and reviewable release behavior. Loading third-party native code into the application would grant process privileges and rely on an unstable Rust/native plugin ABI. A future extension mechanism needs a smaller, enforceable capability boundary.

## Binding decision

> Official MVP packs are compiled in. Future third-party packs use a sandboxed WASM/component model, not native dynamic libraries.

Official packs are explicitly registered. A future third-party host must define a narrow versioned component interface, bounded memory and fuel, signed manifests, and no ambient filesystem or network access.

## Consequences

- Phase 00 and MVP do not claim a third-party pack marketplace or WASM host.
- Official pack code is part of the reviewed application build and normal dependency/license policy.
- The future component boundary requires explicit host calls, resource limits, provenance, compatibility, and trust policy before implementation.
- Domain-pack data remains untrusted even when its code is official.

## Rejected alternatives

- Native dynamic libraries and an unstable Rust plugin ABI are rejected for third-party packs.
- In-process third-party native code is rejected because it cannot provide the intended sandbox boundary.
- Ambient filesystem/network access for future components is rejected.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
