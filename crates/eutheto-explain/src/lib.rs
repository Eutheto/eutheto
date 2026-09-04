//! Pure, deterministic explanation algorithms over validated domain and planning IR.
//!
//! This crate contains no solver, persistence, clock, transport, or presentation authority.
//! Backend-originated data is accepted only after it has been mapped to solver-neutral planning
//! identities, and accepted candidates are compared only through independently verified domain IR.

mod assumption;
mod counterfactual;
mod shrink;

pub use eutheto_domain_ir::{
    ComparisonContext, ComparisonError, ComparisonLockPair, ComparisonRunManifests, ComparisonSide,
    compare_accepted_results,
};

pub use assumption::*;
pub use counterfactual::*;
pub use shrink::*;
