<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-verify`

Independent, solver-neutral candidate acceptance policy.

## Boundary

This crate projects backend values through a compiled-in domain pack, validates the projected
solution against the planning IR, requires complete independent rule evaluation and authoritative
score integrity, and constructs accepted-result data only after every binding passes. Backend
status, generated backend constraints, and backend objective values are never acceptance
authority.

The crate may depend on domain, planning, and solver API contracts. It must not depend on the
router, solver adapters, persistence, application services, Tauri, or presentation code.
