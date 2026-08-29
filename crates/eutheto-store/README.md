<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-store` (reserved)

This directory reserves the roadmap boundary for **future infrastructure adapter implementing application-owned persistence ports.** No `Cargo.toml` is present, so this is not a Cargo workspace member, implemented crate, or published API.

## Boundary

It must not own domain policy or permit callers to bypass validation, revision checks, or transactions.

The owning roadmap phase must confirm that this boundary still warrants a separate crate, choose its dependencies, and add implementation and tests. The exact future crate inventory is an explicit architecture gate; the reserved name alone does not settle it.
