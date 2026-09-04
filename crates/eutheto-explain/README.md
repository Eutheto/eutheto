<!-- SPDX-License-Identifier: Apache-2.0 -->

# `eutheto-explain`

Pure, deterministic explanation algorithms over validated domain and planning IR.

## Boundary

The crate maps solver-neutral assumption literals, performs budget-aware conflict shrinking,
compares independently accepted results, validates additive counterfactual compilations, and
interprets counterfactual terminal records. It has no solver API, persistence, async runtime,
backend launcher, clock implementation, or presentation authority.

It depends only on `eutheto-types`, `eutheto-domain-ir`, and `eutheto-planning-ir`. Backend claims
must first be converted into the validated contracts owned by those crates.
