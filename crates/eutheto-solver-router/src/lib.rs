//! Deterministic Phase-02 solver routing, conservative component evidence, and bounded fallback.
//!
//! This crate does not register a production backend, split a model, merge domain results, or
//! verify candidates. Backends and independent verification authority are injected by callers.

mod decision;
mod execution;
mod profile;

pub use decision::*;
pub use execution::*;
pub use profile::*;

/// Constructs the production metadata registry before platform artifact discovery.
///
/// The generated matrix remains visible, while runnable backends are injected only
/// after their packaged artifacts pass startup validation.
///
/// # Errors
/// Returns a registry error if the generated support-matrix contract is inconsistent.
pub fn production_registry()
-> Result<eutheto_solver_api::SolverRegistry, eutheto_solver_api::RegistryError> {
    eutheto_solver_api::SolverRegistry::production()
}
