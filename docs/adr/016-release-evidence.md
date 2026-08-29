<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-016: Release Evidence and Notices

- **Status:** Approved; release production is a later-phase gate
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)

## Context

A native desktop distribution combines project code, frontend packages, Rust crates, native solver code, runtime libraries, and platform artifacts. Consumers and maintainers need machine-verifiable integrity and an exact record of what was built and licensed. Phase 00 can define this evidence without claiming a releasable product.

## Binding decision

> Releases include checksums, SPDX SBOM, solver/version manifests, and third-party license notices.

Release evidence is generated from locked authoritative inputs. Build and signing jobs are separated, artifacts cross the boundary by verified digest, and protected signing material never enters PR jobs or build derivations.

## Consequences

- Every official release artifact set carries checksums and a matching SPDX software bill of materials.
- Solver manifests identify exact source, build, protocol, capability, linkage, target, and license inputs.
- Third-party notices cover shipped crates, packages, native libraries, code, assets, fonts, data, and models as applicable.
- Generated evidence is never hand-edited and must be reproducible from clean locked inputs.
- Signing, notarization, updater metadata, hosting, and custody identities remain explicit unresolved gates; no placeholder identity is published.

## Rejected alternatives

- Shipping without exact dependency/license inventory is rejected.
- Trusting build-job filenames instead of digest verification at the signing boundary is rejected.
- Embedding signing credentials in repositories, Nix derivations, PR jobs, logs, caches, or ordinary environment setup is rejected.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
