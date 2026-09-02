<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-solver-ortools`

This crate owns deterministic planning-IR-to-CP-SAT translation and the isolated
worker executable identity, launch, and single-session protocol supervisor.
Translation currently covers scalar variable domains plus Boolean-or,
conjunction, implication, equivalence, one-of, cardinality-range, integer linear
comparison constraints, enforcement literals, and safely scalarizable bounded
objective terms, rejecting unsupported or multipass planning features rather than
omitting them. The crate
does not claim a runnable backend until the remaining Phase 03 translation,
result-decoding, and `SolverBackend` work is complete.
