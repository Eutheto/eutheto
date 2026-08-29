<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-solver-api` (reserved)

This directory reserves the roadmap boundary for **future solver capability, request, candidate, progress, cancellation, and error interfaces.** No `Cargo.toml` is present, so this is not a Cargo workspace member, implemented crate, or published API.

## Boundary

It consumes validated solver-neutral planning IR; it does not confer verification or persistence authority on a backend.

The owning roadmap phase must confirm that this boundary still warrants a separate crate, choose its dependencies, and add implementation and tests. The exact future crate inventory is an explicit architecture gate; the reserved name alone does not settle it.
