<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-import` (reserved)

This directory reserves the roadmap boundary for **future bounded parsing and migration of untrusted external scenarios and bundles.** No `Cargo.toml` is present, so this is not a Cargo workspace member, implemented crate, or published API.

## Boundary

It must fully validate before one atomic application-owned commit and must not expose partial imported state as authoritative.

The owning roadmap phase must confirm that this boundary still warrants a separate crate, choose its dependencies, and add implementation and tests. The exact future crate inventory is an explicit architecture gate; the reserved name alone does not settle it.
