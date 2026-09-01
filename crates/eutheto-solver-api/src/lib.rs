//! Stable backend-facing contracts for solver adapters.
//!
//! This crate owns descriptor, support-matrix, preflight, bounded progress/candidate, and
//! backend outcome contracts. It does not select backends, solve models, project candidates,
//! or accept domain results.

mod contracts;
mod descriptor;
pub mod generated_support_matrix;
mod preflight;
mod registry;
mod support;

pub use contracts::*;
pub use descriptor::*;
pub use generated_support_matrix::{
    DEFERRED_BACKEND_CANDIDATES, PRODUCTION_BACKENDS, SUPPORT_FEATURES,
    SUPPORT_MATRIX_IR_SCHEMA_VERSION, SUPPORT_MATRIX_SCHEMA_VERSION,
};
pub use preflight::*;
pub use registry::*;
pub use support::*;
