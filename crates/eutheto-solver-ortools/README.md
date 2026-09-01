<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-solver-ortools`

This crate owns the isolated OR-Tools worker executable identity, launch, and
single-session protocol supervisor. It deliberately does not translate planning
IR into CP-SAT models and does not implement `SolverBackend`; those remain
separate phase boundaries.
