<!-- SPDX-License-Identifier: Apache-2.0 -->

# Workforce `core` boundary

This directory reserves the future Rust domain-pack implementation for the official **workforce planning and scheduling** pack. It contains no implementation, manifest, public format, or compatibility promise in Phase 00.

## Boundary

It may use the domain API, stable value types, solver-neutral planning IR, and normalized verification contracts. It must not depend on Tauri, Vue, SQLite, credential stores, network providers, OR-Tools, Pumpkin, or backend-native objects.

The owning roadmap phase must approve the concrete crate/package inventory and add real behavior with its contracts and tests. Keeping this path does not make it a Cargo or pnpm workspace member.
