<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-command` (reserved)

This directory reserves the roadmap boundary for **future typed application command, query, journal, and undo services.** No `Cargo.toml` is present, so this is not a Cargo workspace member, implemented crate, or published API.

## Boundary

It may coordinate domain and infrastructure ports, but it must not depend on Vue/Tauri presentation details or backend-native solver models.

The owning roadmap phase must confirm that this boundary still warrants a separate crate, choose its dependencies, and add implementation and tests. The exact future crate inventory is an explicit architecture gate; the reserved name alone does not settle it.
