#![forbid(unsafe_code)]

mod fault;
pub mod frame;
pub mod limits;
pub mod state;
pub mod stderr;
pub mod strict;

#[allow(clippy::all, clippy::pedantic)]
pub mod wire {
    include!("generated/eutheto.worker.v1.rs");
}

pub use fault::{FrameFault, ProtocolFault, StateFault, WireFault, WireViolation};
pub use limits::{FrameClass, MAX_ORTOOLS_WORKER_THREADS, ProtocolPolicy, checked_in_policy};
pub use state::{
    HandshakeExpectations, HandshakeRejectionEvidence, NormalizedAppliedParameters, ParentPhase,
    ParentProtocol, StartedObservation, TerminalEvidence, WorkerObservation,
    applied_parameters_preimage, applied_parameters_sha256, normalize_applied_parameters,
};
pub use stderr::{BoundedStderr, STDERR_TRUNCATION_MARKER};
