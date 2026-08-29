<!-- SPDX-License-Identifier: Apache-2.0 -->

# Workforce `ui` boundary

This directory reserves future domain-specific presentation components and view logic for the official **workforce planning and scheduling** pack. It contains no implementation, manifest, public format, or compatibility promise in Phase 00.

## Boundary

It is presentation only: application commands and generated frontend contracts remain the route to Rust authority. It must not own scenario mutation, persistence, scoring, feasibility, worker access, or credentials.

The owning roadmap phase must approve the concrete crate/package inventory and add real behavior with its contracts and tests. Keeping this path does not make it a Cargo or pnpm workspace member.
