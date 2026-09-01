//! Immutable, solver-neutral planning IR schema v1.
//!
//! This crate owns only Boolean/integer/half-open-interval mathematics, provenance,
//! projection, objectives, canonical hashing, bounded validation, and conservative component
//! analysis. It has no backend objects, routing, solver, verifier engine, pack implementation,
//! UI, dynamic plugin, circuit, or path primitive.

mod analysis;
mod canonical;
mod ids;
mod model;
mod projection;
mod validation;

pub use analysis::*;
pub use canonical::*;
pub use ids::*;
pub use model::*;
pub use projection::*;
pub use validation::*;

#[cfg(test)]
mod test_evaluator;
#[cfg(test)]
mod tests;
