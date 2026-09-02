<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-solver-ortools`

This crate owns the manifest-authenticated bundled-worker artifact, exact
matrix-backed compatibility report, deterministic planning-IR-to-CP-SAT
translation, strict candidate decoding, truthful bounded progress conversion,
exact adapter timing and quality evidence aggregation, isolated worker launch and
single-session protocol supervision, and the production OR-Tools
`SolverBackend` registry entry.

Translation currently covers scalar variable domains plus Boolean-or,
conjunction, implication, equivalence, one-of, cardinality-range, integer linear
comparison constraints, enforcement literals, and safely scalarizable bounded
objective terms. It requests every scalar assignment required by the backend
output contract plus domain projections, rejecting unsupported or multipass
planning features rather than omitting them. Returned candidates remain
non-authoritative and require the independent verification boundary owned by
Phase 04.
