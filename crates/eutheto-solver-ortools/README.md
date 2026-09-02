<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-solver-ortools`

This crate owns deterministic planning-IR-to-CP-SAT translation and the isolated
worker executable identity, launch, and single-session protocol supervisor.
Translation currently covers scalar variable domains and unenforced Boolean-or
clauses, rejecting every unsupported planning feature rather than omitting it. The
crate does not claim a runnable backend until the remaining Phase 03 translation,
result-decoding, and `SolverBackend` work is complete.
