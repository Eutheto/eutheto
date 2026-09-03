use std::sync::Arc;
use std::time::{Duration, Instant};

use eutheto_planning_ir::PlanningIrLimitsV1;
use eutheto_protocol::wire::{
    Capability, Progress, ProgressKind, ResourceLimits, SolveParameters,
    SolveRequest as WorkerSolveRequest, worker_frame,
};
use eutheto_protocol::{
    CompletedSession, MAX_ORTOOLS_WORKER_THREADS, WorkerObservation, checked_in_policy,
};
use eutheto_solver_api::{
    BACKEND_STATISTIC_MAX_V1, BackendAppliedParameterEvidence, BackendError,
    BackendExecutionEvidence, BackendModelCountEvidence, BackendOutputSink,
    BackendReproducibilityEvidence, BackendSolveFuture, BackendSolveOutcome,
    BackendTerminationReason, BackendTimingEvidence, BackendWorkerStatistics, CapabilityMatrix,
    CompatibilityLevel, CompatibilityReport, RegistryError, SolveDispatchBudget, SolverBackend,
    SolverDescriptor, SolverRegistry, preflight,
};
use eutheto_types::{DurationMillis, ReproducibilityMode, SolveOptions, WorkerThreadPolicy};
use prost::bytes::Bytes;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    AdapterEvidenceRecorder, CandidateProofClaim, NormalizedWorkerTerminal,
    ORTOOLS_ADAPTER_VERSION, ORTOOLS_VERSION, OrToolsDescriptorError, SafeStopReason,
    SessionCompletion, SessionFailure, SessionRequest, SupervisorError, TranslatedCpSatModel,
    VerifiedWorkerArtifact, WorkerIdentity, WorkerLimitKind, backend_started_progress,
    bound_progress, candidate_progress, candidate_submission, duration_millis, normalize_terminal,
    ortools_compatibility, ortools_descriptor, supervise_with_observer, translate_supported_model,
};

const WORKER_IDENTITY: &str = "eutheto-ortools-worker";
const WORKER_BACKEND_ID: &str = "ortools-cp-sat";
const WORKER_VERSION: &str = "0.1.0";

const WORKER_CAPABILITIES: [Capability; 7] = [
    Capability::CpSat,
    Capability::DeterministicTime,
    Capability::IntermediateSolutions,
    Capability::ObjectiveBounds,
    Capability::Progress,
    Capability::SolutionProjection,
    Capability::SolutionStats,
];

/// The production OR-Tools adapter bound to one manifest-verified worker runtime closure.
pub struct OrToolsBackend {
    descriptor: SolverDescriptor,
    matrix: CapabilityMatrix,
    artifact: VerifiedWorkerArtifact,
}

impl OrToolsBackend {
    /// Constructs the adapter only when its descriptor matches the generated production matrix.
    ///
    /// # Errors
    ///
    /// Returns an error when the reviewed descriptor or generated support matrix is inconsistent.
    pub fn new(
        matrix: CapabilityMatrix,
        artifact: VerifiedWorkerArtifact,
    ) -> Result<Self, OrToolsBackendBuildError> {
        let descriptor = ortools_descriptor()?;
        matrix.validate_descriptor(&descriptor)?;
        Ok(Self {
            descriptor,
            matrix,
            artifact,
        })
    }

    fn worker_identity() -> WorkerIdentity {
        WorkerIdentity {
            worker_identity: WORKER_IDENTITY.to_owned(),
            worker_version: WORKER_VERSION.to_owned(),
            backend_id: WORKER_BACKEND_ID.to_owned(),
            ortools_version: ORTOOLS_VERSION.to_owned(),
            adapter_version: ORTOOLS_ADAPTER_VERSION.to_owned(),
            required_capabilities: WORKER_CAPABILITIES.to_vec(),
            advertised_capabilities: WORKER_CAPABILITIES.to_vec(),
        }
    }

    async fn solve_request(
        &self,
        request: &eutheto_solver_api::SolveRequest,
        output: &mut dyn BackendOutputSink,
    ) -> Result<BackendSolveOutcome, BackendError> {
        preflight(&self.matrix, &self.descriptor, request).map_err(|_| {
            safe_backend_error(
                "solver.ortools.preflight",
                "The routed request failed the shared solver preflight contract.",
            )
        })?;
        let compatibility = self.compatibility(request.summary(), request.options());
        if compatibility.level == CompatibilityLevel::Unsupported {
            return Err(safe_backend_error(
                "solver.ortools.unsupported",
                "The routed request is outside the reviewed OR-Tools capability surface.",
            ));
        }

        let translation_started = Instant::now();
        let translated = translate_supported_model(request.problem(), PlanningIrLimitsV1::DEFAULT)
            .map_err(|_| {
                safe_backend_error("solver.ortools.translation", "The validated planning model could not be translated by the reviewed OR-Tools adapter.")
            })?;
        let (worker_request, request_evidence) = build_worker_request(request, &translated)?;
        let translation_milliseconds = duration_millis(translation_started.elapsed());
        let mut recorder = AdapterEvidenceRecorder::new(
            &translated,
            request.dispatch_budget().remaining_at_dispatch(),
            request.dispatch_budget().backend_limit(),
        );

        let session = SessionRequest {
            executable: self.artifact.executable(),
            identity: Self::worker_identity(),
            core_version: env!("CARGO_PKG_VERSION").to_owned(),
            solve: worker_request,
        };
        let completion = supervise_with_observer(
            session,
            request.dispatch_budget().child_view(),
            |observation| {
                observe_worker(
                    observation,
                    &translated,
                    request.dispatch_budget(),
                    output,
                    &mut recorder,
                )
            },
        )
        .await;

        let execution_evidence = self.execution_evidence(
            request,
            &translated,
            &request_evidence,
            translation_milliseconds,
            &completion,
        )?;
        let termination = match &completion {
            Ok(completion) => {
                process_terminal_candidate(
                    completion,
                    &translated,
                    request,
                    output,
                    &mut recorder,
                )?;
                normalized_termination(normalize_terminal(&completion.evidence))
            }
            Err(failure) => failure_termination(failure),
        };
        let mut evidence = recorder
            .finish(request.dispatch_budget().elapsed_milliseconds())
            .map_err(|_| {
                safe_backend_error(
                    "solver.ortools.evidence",
                    "OR-Tools adapter evidence violated its parent-measured ordering contract.",
                )
            })?;
        evidence.execution = Some(execution_evidence);
        Ok(BackendSolveOutcome {
            backend_id: self.descriptor.id.clone(),
            model_hash: request.model_hash().to_owned(),
            solve_fingerprint: request.solve_fingerprint().to_owned(),
            termination,
            evidence,
        })
    }

    fn execution_evidence(
        &self,
        request: &eutheto_solver_api::SolveRequest,
        translated: &TranslatedCpSatModel,
        request_evidence: &WorkerRequestEvidence,
        translation_milliseconds: DurationMillis,
        session: &Result<SessionCompletion, SessionFailure>,
    ) -> Result<BackendExecutionEvidence, BackendError> {
        let policy = checked_in_policy().map_err(|_| {
            safe_backend_error(
                "solver.ortools.protocol_policy",
                "The checked-in worker protocol policy is invalid.",
            )
        })?;
        let session_timings = match session {
            Ok(completion) => Some(&completion.timings),
            Err(failure) => failure.timings.as_ref(),
        };
        let translation_milliseconds = session_timings
            .and_then(|value| value.request_dispatch_milliseconds)
            .map_or(translation_milliseconds, |request_dispatch| {
                DurationMillis::new(
                    translation_milliseconds
                        .value()
                        .saturating_add(request_dispatch.value()),
                )
                .unwrap_or(DurationMillis::MAX)
            });
        let timings = BackendTimingEvidence {
            translation_serialization_milliseconds: translation_milliseconds,
            worker_startup_milliseconds: session_timings.map(|value| value.startup_milliseconds),
            handshake_milliseconds: session_timings.map(|value| value.handshake_milliseconds),
            solver_milliseconds: session_timings.and_then(|value| value.solver_milliseconds),
            protocol_decode_milliseconds: session_timings
                .map(|value| value.protocol_decode_milliseconds),
        };
        let (worker_statistics, applied_parameters_sha256) =
            terminal_execution_evidence(session.as_ref().ok())?;
        let mut applied_parameters = request_evidence.applied_parameters.clone();
        applied_parameters.wall_time_milliseconds =
            session_timings.and_then(|value| value.dispatched_wall_time_milliseconds);
        Ok(BackendExecutionEvidence {
            timings,
            model_counts: BackendModelCountEvidence {
                planning_variable_count: request.summary().variable_count,
                planning_constraint_count: request.summary().constraint_count,
                translated_variable_count: translated.translated_variable_count(),
                translated_constraint_count: translated.translated_constraint_count(),
            },
            worker_statistics,
            reproducibility: BackendReproducibilityEvidence {
                backend_version: self.descriptor.version.clone(),
                adapter_version: self.descriptor.adapter_version.clone(),
                worker_version: WORKER_VERSION.to_owned(),
                engine_version: ORTOOLS_VERSION.to_owned(),
                protocol_major: policy.protocol_major(),
                protocol_minor: policy.protocol_minor(),
                applied_options: request.options().clone(),
                applied_parameters,
                model_fingerprint_sha256: request_evidence.model_fingerprint_sha256.clone(),
                applied_parameters_sha256,
            },
        })
    }
}

impl SolverBackend for OrToolsBackend {
    fn descriptor(&self) -> &SolverDescriptor {
        &self.descriptor
    }

    fn compatibility(
        &self,
        problem: &eutheto_planning_ir::PlanningProblemSummary,
        options: &SolveOptions,
    ) -> CompatibilityReport {
        ortools_compatibility(&self.matrix, problem, options).unwrap_or_else(|_| {
            CompatibilityReport {
                level: CompatibilityLevel::Unsupported,
                unsupported_features: Vec::new(),
                warnings: Vec::new(),
                estimated_translation_cost: None,
            }
        })
    }

    fn solve<'a>(
        &'a self,
        request: &'a eutheto_solver_api::SolveRequest,
        output: &'a mut dyn BackendOutputSink,
    ) -> BackendSolveFuture<'a> {
        Box::pin(self.solve_request(request, output))
    }
}

/// Builds the exact production registry around one already manifest-verified bundled worker.
///
/// # Errors
///
/// Returns an error when generated metadata, the OR-Tools descriptor, or registry invariants fail.
pub fn registry_with_ortools(
    artifact: VerifiedWorkerArtifact,
) -> Result<SolverRegistry, OrToolsBackendBuildError> {
    let matrix = CapabilityMatrix::generated()?;
    let backend = Arc::new(OrToolsBackend::new(matrix.clone(), artifact)?);
    let backend: Arc<dyn SolverBackend> = backend;
    Ok(SolverRegistry::new(matrix, [backend])?)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkerRequestEvidence {
    applied_parameters: BackendAppliedParameterEvidence,
    model_fingerprint_sha256: String,
}

fn build_worker_request(
    request: &eutheto_solver_api::SolveRequest,
    translated: &TranslatedCpSatModel,
) -> Result<(WorkerSolveRequest, WorkerRequestEvidence), BackendError> {
    let random_seed = i32::try_from(request.options().random_seed).map_err(|_| {
        safe_backend_error(
            "solver.ortools.random_seed",
            "The random seed is outside the OR-Tools signed 32-bit parameter range.",
        )
    })?;
    let deterministic = request.options().reproducibility == ReproducibilityMode::Deterministic;
    let log_search_progress = !translated.objective_plan().levels.is_empty();
    let worker_threads = match request.options().worker_threads {
        WorkerThreadPolicy::Exact(count) => u32::from(count),
        WorkerThreadPolicy::Auto => std::thread::available_parallelism()
            .map_or(1, |count| u32::try_from(count.get()).unwrap_or(u32::MAX))
            .clamp(1, MAX_ORTOOLS_WORKER_THREADS),
    };
    let model_fingerprint: [u8; 32] = Sha256::digest(translated.cp_model_proto()).into();
    let wall_time_millis = u64::try_from(
        request
            .dispatch_budget()
            .remaining_backend_duration()
            .as_millis(),
    )
    .unwrap_or(u64::MAX);

    let worker_request = WorkerSolveRequest {
        request_id: request.solve_fingerprint().to_owned(),
        cp_model_proto: Bytes::copy_from_slice(translated.cp_model_proto()),
        parameters: Some(SolveParameters {
            random_seed: Some(random_seed),
            stop_after_first_feasible: Some(request.options().stop_after_first_feasible),
            emit_intermediate_solutions: Some(false),
            log_search_progress: Some(log_search_progress),
            deterministic_test_profile: Some(deterministic),
        }),
        projections: translated.worker_projection_requests().to_vec(),
        resource_limits: Some(ResourceLimits {
            wall_time_millis,
            memory_bytes: None,
            worker_threads,
        }),
        model_fingerprint: Bytes::copy_from_slice(&model_fingerprint),
    };
    Ok((
        worker_request,
        WorkerRequestEvidence {
            applied_parameters: BackendAppliedParameterEvidence {
                wall_time_milliseconds: None,
                memory_limit_bytes: None,
                worker_threads,
                random_seed,
                stop_after_first_feasible: request.options().stop_after_first_feasible,
                emit_intermediate_solutions: false,
                log_search_progress,
                deterministic_test_profile: deterministic,
            },
            model_fingerprint_sha256: encode_hex(&model_fingerprint),
        },
    ))
}

fn terminal_execution_evidence(
    completion: Option<&SessionCompletion>,
) -> Result<(Option<BackendWorkerStatistics>, Option<String>), BackendError> {
    let Some(SessionCompletion {
        evidence: CompletedSession::Solve(terminal),
        ..
    }) = completion
    else {
        return Ok((None, None));
    };
    let Some(worker_frame::Body::Finished(finished)) = &terminal.frame().body else {
        return Ok((None, None));
    };
    if finished.applied_parameters_sha256.len() != 32 {
        return Err(safe_backend_error(
            "solver.ortools.parameters_hash",
            "OR-Tools returned an invalid applied-parameter digest.",
        ));
    }
    Ok((
        Some(BackendWorkerStatistics {
            wall_time_milliseconds: seconds_to_millis(finished.wall_time_seconds)?,
            user_time_milliseconds: seconds_to_millis(finished.user_time_seconds)?,
            deterministic_time_milliseconds: seconds_to_millis(finished.deterministic_time)?,
            conflicts: checked_worker_counter(finished.conflicts)?,
            branches: checked_worker_counter(finished.branches)?,
            binary_propagations: checked_worker_counter(finished.binary_propagations)?,
            integer_propagations: checked_worker_counter(finished.integer_propagations)?,
        }),
        Some(encode_hex(&finished.applied_parameters_sha256)),
    ))
}

fn seconds_to_millis(value: Option<f64>) -> Result<Option<DurationMillis>, BackendError> {
    value
        .map(|seconds| {
            let duration = Duration::try_from_secs_f64(seconds).map_err(|_| {
                safe_backend_error(
                    "solver.ortools.worker_timing",
                    "OR-Tools returned an invalid native timing value.",
                )
            })?;
            let milliseconds = u64::try_from(duration.as_millis()).map_err(|_| {
                safe_backend_error(
                    "solver.ortools.worker_timing",
                    "OR-Tools returned an out-of-range native timing value.",
                )
            })?;
            DurationMillis::new(milliseconds).map_err(|_| {
                safe_backend_error(
                    "solver.ortools.worker_timing",
                    "OR-Tools returned an out-of-range native timing value.",
                )
            })
        })
        .transpose()
}

fn checked_worker_counter(value: Option<u64>) -> Result<Option<u64>, BackendError> {
    if value.is_some_and(|count| count > BACKEND_STATISTIC_MAX_V1) {
        return Err(safe_backend_error(
            "solver.ortools.worker_statistics",
            "OR-Tools returned an out-of-range native search counter.",
        ));
    }
    Ok(value)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn observe_worker(
    observation: WorkerObservation,
    translated: &TranslatedCpSatModel,
    dispatch_budget: &SolveDispatchBudget,
    output: &mut dyn BackendOutputSink,
    recorder: &mut AdapterEvidenceRecorder,
) -> Result<(), SupervisorError> {
    match observation {
        WorkerObservation::Started(_) => output
            .emit_progress(backend_started_progress().map_err(|_| SupervisorError::AdapterOutput)?)
            .map_err(|_| SupervisorError::AdapterOutput),
        WorkerObservation::Progress(progress) => {
            let elapsed = dispatch_budget.elapsed_milliseconds();
            if let Some(event) = bound_progress(translated, &progress, elapsed)
                .map_err(|_| SupervisorError::AdapterOutput)?
            {
                let emit =
                    if let eutheto_solver_api::SolveProgressEvent::BoundImproved(bound) = &event {
                        recorder
                            .record_bound(bound)
                            .map_err(|_| SupervisorError::AdapterOutput)?
                    } else {
                        true
                    };
                if emit {
                    output
                        .emit_progress(event)
                        .map_err(|_| SupervisorError::AdapterOutput)?;
                }
            }
            Ok(())
        }
        WorkerObservation::Incumbent(_) => Err(SupervisorError::AdapterOutput),
        WorkerObservation::Terminal => Ok(()),
        WorkerObservation::HandshakeAccepted | WorkerObservation::HandshakeRejected => {
            Err(SupervisorError::AdapterOutput)
        }
    }
}

fn process_terminal_candidate(
    completion: &SessionCompletion,
    translated: &TranslatedCpSatModel,
    request: &eutheto_solver_api::SolveRequest,
    output: &mut dyn BackendOutputSink,
    recorder: &mut AdapterEvidenceRecorder,
) -> Result<(), BackendError> {
    let CompletedSession::Solve(terminal) = &completion.evidence else {
        return Ok(());
    };
    let Some(worker_frame::Body::Finished(finished)) = &terminal.frame().body else {
        return Ok(());
    };
    let elapsed = request.dispatch_budget().elapsed_milliseconds();
    let mut staged_recorder = recorder.clone();
    let mut terminal_bound = None;
    if !finished.best_bound_values.is_empty() {
        let progress = Progress {
            kind: ProgressKind::BoundImproved as i32,
            best_bound_values: finished.best_bound_values.clone(),
            ..Progress::default()
        };
        if let Some(event) = bound_progress(translated, &progress, elapsed).map_err(|_| {
            safe_backend_error(
                "solver.ortools.bound",
                "OR-Tools returned invalid terminal bound evidence.",
            )
        })? {
            let emit = if let eutheto_solver_api::SolveProgressEvent::BoundImproved(bound) = &event
            {
                staged_recorder.record_bound(bound).map_err(|_| {
                    safe_backend_error(
                        "solver.ortools.bound",
                        "OR-Tools terminal bound evidence contradicted the translated objective.",
                    )
                })?
            } else {
                true
            };
            if emit {
                terminal_bound = Some(event);
            }
        }
    }
    let terminal_candidate =
        if let Some(candidate) = &finished.final_candidate {
            let submission = candidate_submission(translated, candidate, elapsed).map_err(|_| {
            safe_backend_error(
                "solver.ortools.candidate",
                "OR-Tools returned a projected candidate outside the translated model contract.",
            )
        })?;
            staged_recorder.record_submission(&submission).map_err(|_| {
            safe_backend_error(
                "solver.ortools.evidence",
                "OR-Tools candidate evidence contradicted the translated objective contract.",
            )
        })?;
            Some(submission)
        } else {
            None
        };

    if let Some(event) = terminal_bound {
        output.emit_progress(event)?;
    }
    if let Some(submission) = terminal_candidate {
        let accepted = output.submit_candidate(submission)?;
        output.emit_progress(candidate_progress(&accepted))?;
    }
    *recorder = staged_recorder;
    Ok(())
}

const fn normalized_termination(terminal: NormalizedWorkerTerminal) -> BackendTerminationReason {
    match terminal {
        NormalizedWorkerTerminal::Candidate {
            proof: CandidateProofClaim::Optimal,
            ..
        } => BackendTerminationReason::OptimalityClaimed,
        NormalizedWorkerTerminal::Candidate { .. } => BackendTerminationReason::CandidateFound,
        NormalizedWorkerTerminal::Infeasible => BackendTerminationReason::InfeasibilityClaimed,
        NormalizedWorkerTerminal::NoSolutionWithinLimit(WorkerLimitKind::Solution) => {
            BackendTerminationReason::SolutionLimit
        }
        NormalizedWorkerTerminal::NoSolutionWithinLimit(_) => BackendTerminationReason::TimeLimit,
        NormalizedWorkerTerminal::Cancelled { .. } => BackendTerminationReason::Cancelled,
        NormalizedWorkerTerminal::AdapterDefect(_) => BackendTerminationReason::InvalidModel,
        NormalizedWorkerTerminal::BackendUnavailable(_) => BackendTerminationReason::Unavailable,
        NormalizedWorkerTerminal::BackendFailed { .. } => BackendTerminationReason::Failed,
    }
}

const fn failure_termination(failure: &SessionFailure) -> BackendTerminationReason {
    match failure.stop_reason {
        SafeStopReason::Cancelled => BackendTerminationReason::Cancelled,
        SafeStopReason::DeadlineExceeded => BackendTerminationReason::TimeLimit,
        SafeStopReason::HandshakeRejected | SafeStopReason::Io => {
            BackendTerminationReason::Unavailable
        }
        SafeStopReason::Completed
        | SafeStopReason::ProtocolViolation
        | SafeStopReason::OutputLimit
        | SafeStopReason::WorkerExit => BackendTerminationReason::Failed,
    }
}

fn safe_backend_error(code: &'static str, message: &'static str) -> BackendError {
    match BackendError::new(code, message) {
        Ok(error) => error,
        Err(error) => BackendError::from(error),
    }
}

/// Failure to construct the exact production OR-Tools backend registry.
#[derive(Debug, Error)]
pub enum OrToolsBackendBuildError {
    #[error(transparent)]
    Descriptor(#[from] OrToolsDescriptorError),
    #[error(transparent)]
    Matrix(#[from] eutheto_solver_api::SupportMatrixError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
}
