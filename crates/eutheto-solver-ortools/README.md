<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-solver-ortools`

This crate owns deterministic planning-IR-to-CP-SAT translation and the isolated
worker executable identity, launch, and single-session protocol supervisor.
Translation currently exposes only its completed variable-domain stage; it does
not claim a runnable backend until the remaining Phase 03 constraint, objective,
projection, result-decoding, and `SolverBackend` work is complete.
