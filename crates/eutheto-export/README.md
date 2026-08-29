<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-export` (reserved)

This directory reserves the roadmap boundary for **future canonical, versioned scenario and bundle export at an application boundary.** No `Cargo.toml` is present, so this is not a Cargo workspace member, implemented crate, or published API.

## Boundary

It must not leak credentials or private diagnostics, and it must not expose a backend-native model as the public format.

The owning roadmap phase must confirm that this boundary still warrants a separate crate, choose its dependencies, and add implementation and tests. The exact future crate inventory is an explicit architecture gate; the reserved name alone does not settle it.
