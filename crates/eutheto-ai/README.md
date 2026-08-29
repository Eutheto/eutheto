<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-ai` (reserved)

This directory reserves the roadmap boundary for **future optional AI adapter for bounded context and typed proposals.** No `Cargo.toml` is present, so this is not a Cargo workspace member, implemented crate, or published API.

## Boundary

It must not access persistence, credentials, arbitrary files, shell/code execution, or solver, verifier, routing, or mutation authority.

The owning roadmap phase must confirm that this boundary still warrants a separate crate, choose its dependencies, and add implementation and tests. The exact future crate inventory is an explicit architecture gate; the reserved name alone does not settle it.
