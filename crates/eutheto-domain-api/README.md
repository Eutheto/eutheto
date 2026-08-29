<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-domain-api` (reserved)

This directory reserves the roadmap boundary for **future domain-pack interfaces and registration contracts.** No `Cargo.toml` is present, so this is not a Cargo workspace member, implemented crate, or published API.

## Boundary

It may depend on stable shared value types, but it must not expose persistence, desktop, provider, or solver-backend details.

The owning roadmap phase must confirm that this boundary still warrants a separate crate, choose its dependencies, and add implementation and tests. The exact future crate inventory is an explicit architecture gate; the reserved name alone does not settle it.
