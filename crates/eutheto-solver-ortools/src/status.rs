use eutheto_protocol::CompletedSession;
use eutheto_protocol::wire::{
    Finished, TerminationReason, WorkerError, WorkerErrorCode, WorkerSolveStatus, worker_frame,
};

/// Solver proof attached to a projected candidate before independent verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateProofClaim {
    /// CP-SAT claims optimality for its translated model and encoded objective.
    Optimal,
    /// CP-SAT found a candidate without proving optimality.
    Feasible,
}

/// Exact worker-side limit that stopped a solve.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerLimitKind {
    Time,
    Solution,
    Memory,
    Resource,
}

/// Why a candidate-bearing solve stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateStopCause {
    Optimal,
    Limit(WorkerLimitKind),
}

/// Worker outcomes that indicate an adapter/compiler defect rather than a user-domain result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterDefectKind {
    InvalidTranslatedModel,
    UnsupportedTranslatedModel,
    InvalidAppliedParameters,
}

/// Safe reason a worker was unavailable before returning a domain-independent result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerUnavailableKind {
    HandshakeRejected,
    OrToolsInitialization,
}

/// Safe classification of a worker failure. Worker diagnostics remain separately bounded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerFailureKind {
    MalformedFrame,
    ProtocolViolation,
    Internal,
    UnknownTermination,
    ProtocolInvariant,
}

/// A protocol-validated worker terminal condition before projection or independent verification.
///
/// Candidate-bearing variants remain untrusted. In particular, [`CandidateProofClaim::Optimal`]
/// is only a backend proof claim for the translated CP-SAT model; it is not domain acceptance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalizedWorkerTerminal {
    Candidate {
        proof: CandidateProofClaim,
        stopped_by: CandidateStopCause,
    },
    Infeasible,
    NoSolutionWithinLimit(WorkerLimitKind),
    Cancelled {
        candidate_available: bool,
    },
    AdapterDefect(AdapterDefectKind),
    BackendUnavailable(WorkerUnavailableKind),
    BackendFailed {
        kind: WorkerFailureKind,
        retryable: bool,
    },
}

/// Classifies a fully protocol-validated completion without trusting candidate or objective data.
#[must_use]
pub fn normalize_terminal(completion: &CompletedSession) -> NormalizedWorkerTerminal {
    match completion {
        CompletedSession::HandshakeRejected(_) => {
            NormalizedWorkerTerminal::BackendUnavailable(WorkerUnavailableKind::HandshakeRejected)
        }
        CompletedSession::Solve(evidence) => match &evidence.frame().body {
            Some(worker_frame::Body::Finished(finished)) => normalize_finished(finished),
            Some(worker_frame::Body::Error(error)) => normalize_worker_error(error),
            _ => NormalizedWorkerTerminal::BackendFailed {
                kind: WorkerFailureKind::ProtocolInvariant,
                retryable: false,
            },
        },
    }
}

fn normalize_finished(finished: &Finished) -> NormalizedWorkerTerminal {
    let status = WorkerSolveStatus::try_from(finished.status);
    let termination = TerminationReason::try_from(finished.termination_reason);
    let candidate_available = finished.final_candidate.is_some();

    match (status, termination) {
        (Ok(WorkerSolveStatus::Optimal), Ok(TerminationReason::Optimal)) => {
            NormalizedWorkerTerminal::Candidate {
                proof: CandidateProofClaim::Optimal,
                stopped_by: CandidateStopCause::Optimal,
            }
        }
        (Ok(WorkerSolveStatus::Feasible), Ok(TerminationReason::TimeLimit)) => {
            feasible_at_limit(WorkerLimitKind::Time)
        }
        (Ok(WorkerSolveStatus::Feasible), Ok(TerminationReason::SolutionLimit)) => {
            feasible_at_limit(WorkerLimitKind::Solution)
        }
        (Ok(WorkerSolveStatus::Feasible), Ok(TerminationReason::MemoryLimit)) => {
            feasible_at_limit(WorkerLimitKind::Memory)
        }
        (Ok(WorkerSolveStatus::Feasible), Ok(TerminationReason::Cancelled)) => {
            NormalizedWorkerTerminal::Cancelled {
                candidate_available: true,
            }
        }
        (Ok(WorkerSolveStatus::Infeasible), Ok(TerminationReason::Infeasible)) => {
            NormalizedWorkerTerminal::Infeasible
        }
        (Ok(WorkerSolveStatus::NoSolution), Ok(TerminationReason::TimeLimit)) => {
            NormalizedWorkerTerminal::NoSolutionWithinLimit(WorkerLimitKind::Time)
        }
        (Ok(WorkerSolveStatus::NoSolution), Ok(TerminationReason::MemoryLimit)) => {
            NormalizedWorkerTerminal::NoSolutionWithinLimit(WorkerLimitKind::Memory)
        }
        (Ok(WorkerSolveStatus::Cancelled), Ok(TerminationReason::Cancelled)) => {
            NormalizedWorkerTerminal::Cancelled {
                candidate_available,
            }
        }
        (Ok(WorkerSolveStatus::InvalidModel), Ok(TerminationReason::InvalidModel)) => {
            NormalizedWorkerTerminal::AdapterDefect(AdapterDefectKind::InvalidTranslatedModel)
        }
        (Ok(WorkerSolveStatus::Failed), Ok(TerminationReason::InternalError)) => {
            NormalizedWorkerTerminal::BackendFailed {
                kind: WorkerFailureKind::Internal,
                retryable: false,
            }
        }
        (Ok(WorkerSolveStatus::NoSolution), Ok(TerminationReason::Unknown)) => {
            NormalizedWorkerTerminal::BackendFailed {
                kind: WorkerFailureKind::UnknownTermination,
                retryable: false,
            }
        }
        _ => NormalizedWorkerTerminal::BackendFailed {
            kind: WorkerFailureKind::ProtocolInvariant,
            retryable: false,
        },
    }
}

const fn feasible_at_limit(limit: WorkerLimitKind) -> NormalizedWorkerTerminal {
    NormalizedWorkerTerminal::Candidate {
        proof: CandidateProofClaim::Feasible,
        stopped_by: CandidateStopCause::Limit(limit),
    }
}

fn normalize_worker_error(error: &WorkerError) -> NormalizedWorkerTerminal {
    match WorkerErrorCode::try_from(error.code) {
        Ok(WorkerErrorCode::MalformedFrame) => {
            backend_failure(WorkerFailureKind::MalformedFrame, error.retryable)
        }
        Ok(WorkerErrorCode::ProtocolViolation) => {
            backend_failure(WorkerFailureKind::ProtocolViolation, error.retryable)
        }
        Ok(WorkerErrorCode::UnsupportedModel) => {
            NormalizedWorkerTerminal::AdapterDefect(AdapterDefectKind::UnsupportedTranslatedModel)
        }
        Ok(WorkerErrorCode::InvalidModel) => {
            NormalizedWorkerTerminal::AdapterDefect(AdapterDefectKind::InvalidTranslatedModel)
        }
        Ok(WorkerErrorCode::InvalidParameters) => {
            NormalizedWorkerTerminal::AdapterDefect(AdapterDefectKind::InvalidAppliedParameters)
        }
        Ok(WorkerErrorCode::ResourceLimit) => {
            NormalizedWorkerTerminal::NoSolutionWithinLimit(WorkerLimitKind::Resource)
        }
        Ok(WorkerErrorCode::OrtoolsInitialization) => NormalizedWorkerTerminal::BackendUnavailable(
            WorkerUnavailableKind::OrToolsInitialization,
        ),
        Ok(WorkerErrorCode::Internal) => {
            backend_failure(WorkerFailureKind::Internal, error.retryable)
        }
        Ok(WorkerErrorCode::Unspecified) | Err(_) => {
            backend_failure(WorkerFailureKind::ProtocolInvariant, false)
        }
    }
}

const fn backend_failure(kind: WorkerFailureKind, retryable: bool) -> NormalizedWorkerTerminal {
    NormalizedWorkerTerminal::BackendFailed { kind, retryable }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eutheto_protocol::wire::ProjectedCandidate;

    fn finished(
        status: WorkerSolveStatus,
        termination: TerminationReason,
        candidate_available: bool,
    ) -> Finished {
        Finished {
            status: status as i32,
            termination_reason: termination as i32,
            final_candidate: candidate_available.then(ProjectedCandidate::default),
            ..Finished::default()
        }
    }

    fn worker_error(code: WorkerErrorCode, retryable: bool) -> WorkerError {
        WorkerError {
            code: code as i32,
            retryable,
            ..WorkerError::default()
        }
    }

    #[test]
    fn only_optimal_terminal_claims_optimality() {
        assert_eq!(
            normalize_finished(&finished(
                WorkerSolveStatus::Optimal,
                TerminationReason::Optimal,
                true,
            )),
            NormalizedWorkerTerminal::Candidate {
                proof: CandidateProofClaim::Optimal,
                stopped_by: CandidateStopCause::Optimal,
            }
        );

        for termination in [
            TerminationReason::TimeLimit,
            TerminationReason::SolutionLimit,
            TerminationReason::MemoryLimit,
        ] {
            let normalized =
                normalize_finished(&finished(WorkerSolveStatus::Feasible, termination, true));
            assert!(matches!(
                normalized,
                NormalizedWorkerTerminal::Candidate {
                    proof: CandidateProofClaim::Feasible,
                    ..
                }
            ));
        }
    }

    #[test]
    fn limit_and_candidate_presence_remain_distinct() {
        for (termination, limit) in [
            (TerminationReason::TimeLimit, WorkerLimitKind::Time),
            (TerminationReason::MemoryLimit, WorkerLimitKind::Memory),
        ] {
            assert_eq!(
                normalize_finished(&finished(WorkerSolveStatus::NoSolution, termination, false,)),
                NormalizedWorkerTerminal::NoSolutionWithinLimit(limit)
            );
        }

        assert_eq!(
            normalize_finished(&finished(
                WorkerSolveStatus::Feasible,
                TerminationReason::SolutionLimit,
                true,
            )),
            NormalizedWorkerTerminal::Candidate {
                proof: CandidateProofClaim::Feasible,
                stopped_by: CandidateStopCause::Limit(WorkerLimitKind::Solution),
            }
        );
    }

    #[test]
    fn conclusive_and_cancelled_finished_statuses_are_truthful() {
        assert_eq!(
            normalize_finished(&finished(
                WorkerSolveStatus::Infeasible,
                TerminationReason::Infeasible,
                false,
            )),
            NormalizedWorkerTerminal::Infeasible
        );
        assert_eq!(
            normalize_finished(&finished(
                WorkerSolveStatus::Cancelled,
                TerminationReason::Cancelled,
                false,
            )),
            NormalizedWorkerTerminal::Cancelled {
                candidate_available: false,
            }
        );
        assert_eq!(
            normalize_finished(&finished(
                WorkerSolveStatus::Cancelled,
                TerminationReason::Cancelled,
                true,
            )),
            NormalizedWorkerTerminal::Cancelled {
                candidate_available: true,
            }
        );
        assert_eq!(
            normalize_finished(&finished(
                WorkerSolveStatus::Feasible,
                TerminationReason::Cancelled,
                true,
            )),
            NormalizedWorkerTerminal::Cancelled {
                candidate_available: true,
            }
        );
    }

    #[test]
    fn invalid_translation_is_an_adapter_defect() {
        assert_eq!(
            normalize_finished(&finished(
                WorkerSolveStatus::InvalidModel,
                TerminationReason::InvalidModel,
                false,
            )),
            NormalizedWorkerTerminal::AdapterDefect(AdapterDefectKind::InvalidTranslatedModel,)
        );

        for (code, defect) in [
            (
                WorkerErrorCode::UnsupportedModel,
                AdapterDefectKind::UnsupportedTranslatedModel,
            ),
            (
                WorkerErrorCode::InvalidModel,
                AdapterDefectKind::InvalidTranslatedModel,
            ),
            (
                WorkerErrorCode::InvalidParameters,
                AdapterDefectKind::InvalidAppliedParameters,
            ),
        ] {
            assert_eq!(
                normalize_worker_error(&worker_error(code, false)),
                NormalizedWorkerTerminal::AdapterDefect(defect)
            );
        }
    }

    #[test]
    fn unknown_status_never_becomes_no_solution_within_limit() {
        assert_eq!(
            normalize_finished(&finished(
                WorkerSolveStatus::NoSolution,
                TerminationReason::Unknown,
                false,
            )),
            NormalizedWorkerTerminal::BackendFailed {
                kind: WorkerFailureKind::UnknownTermination,
                retryable: false,
            }
        );
    }

    #[test]
    fn worker_errors_preserve_safe_retry_and_limit_semantics() {
        assert_eq!(
            normalize_worker_error(&worker_error(WorkerErrorCode::ResourceLimit, true)),
            NormalizedWorkerTerminal::NoSolutionWithinLimit(WorkerLimitKind::Resource)
        );
        assert_eq!(
            normalize_worker_error(&worker_error(WorkerErrorCode::OrtoolsInitialization, true,)),
            NormalizedWorkerTerminal::BackendUnavailable(
                WorkerUnavailableKind::OrToolsInitialization,
            )
        );
        assert_eq!(
            normalize_worker_error(&worker_error(WorkerErrorCode::Internal, true)),
            NormalizedWorkerTerminal::BackendFailed {
                kind: WorkerFailureKind::Internal,
                retryable: true,
            }
        );
        assert_eq!(
            normalize_worker_error(&worker_error(WorkerErrorCode::ProtocolViolation, false)),
            NormalizedWorkerTerminal::BackendFailed {
                kind: WorkerFailureKind::ProtocolViolation,
                retryable: false,
            }
        );
    }
}
