<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-solver-ortools`

This crate owns the bundled-worker descriptor, exact matrix-backed compatibility
report, deterministic planning-IR-to-CP-SAT translation, strict projected
candidate decoding, and the isolated worker executable identity, launch, and
single-session protocol supervisor.
Translation currently covers scalar variable domains plus Boolean-or,
conjunction, implication, equivalence, one-of, cardinality-range, integer linear
comparison constraints, enforcement literals, safely scalarizable bounded
objective terms, and minimal candidate projection requests, rejecting unsupported
or multipass planning features rather than omitting them. The crate
does not claim a runnable backend until the remaining Phase 03 `SolverBackend`
integration is complete.
