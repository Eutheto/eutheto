<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-protocol` (reserved)

This directory reserves the roadmap boundary for **future Rust ownership for the versioned solver-worker protocol and its bounded generated types.** No `Cargo.toml` is present, so this is not a Cargo workspace member, implemented crate, or published API.

## Boundary

It must not turn backend status into authoritative feasibility or scoring, and generated sources must come from the protocol authority rather than hand edits.

The owning roadmap phase must confirm that this boundary still warrants a separate crate, choose its dependencies, and add implementation and tests. The exact future crate inventory is an explicit architecture gate; the reserved name alone does not settle it.
