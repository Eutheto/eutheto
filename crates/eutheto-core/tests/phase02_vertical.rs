use eutheto_command::{OFFICIAL_TEST_PACK_ID, official_registry};
use eutheto_domain_api::{CompileContext, DomainPack};
use eutheto_domain_ir::{AssignmentValue, NormalizedSolution, ScoreVector, VerificationReport};
use eutheto_planning_ir::{
    CandidateValues, PlanningIrLimitsV1, PlanningProblem, PlanningProblemSummary, Variable,
    canonical_ir_hash, summarize, validate,
};
use eutheto_solver_api::*;
use eutheto_solver_router::*;
use eutheto_types::{
    BackendId, BackendSelection, CancellationToken, DurationMillis, ExplanationMode,
    FixedMonotonicClock, PackId, ParentSolveBudget, PreservationPolicy, ReproducibilityMode,
    ResourceLimits, ScenarioDocument, SolutionId, SolveMode, SolveOptions, SolveStatus,
    WorkerThreadPolicy,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

const SCENARIO_ID: &str = "0195a5e4-7c00-7000-8000-000000000021";
const ENTITY_ID: &str = "018f25a7-8b3c-7d11-8000-000000000021";
const SOLUTION_ID: &str = "0195a5e4-7c00-7000-8000-000000000022";
const EXACT_BACKEND_ID: &str = "tests.phase02-exact";
const INCOMPATIBLE_BACKEND_ID: &str = "tests.phase02-incompatible";
const UNSUPPORTED_FEATURE_ID: &str = "ir.objective-penalty";
const UNSUPPORTED_REASON: &str = "The synthetic incompatible backend omits objective penalties.";
const UNSUPPORTED_REMEDIATION: &str = "Select the compatible exact Phase-02 test backend.";
const RAW_BACKEND_OBJECTIVE: i64 = 999;

type TestResult = Result<(), Box<dyn Error>>;

struct ExactTestBackend {
    descriptor: SolverDescriptor,
    matrix: CapabilityMatrix,
    invocations: AtomicU32,
}

impl SolverBackend for ExactTestBackend {
    fn descriptor(&self) -> &SolverDescriptor {
        &self.descriptor
    }

    fn compatibility(
        &self,
        summary: &PlanningProblemSummary,
        options: &SolveOptions,
    ) -> CompatibilityReport {
        compatibility_for(&self.matrix, &self.descriptor.id, summary, options).unwrap_or_else(
            |_| CompatibilityReport {
                level: CompatibilityLevel::Unsupported,
                unsupported_features: Vec::new(),
                warnings: Vec::new(),
                estimated_translation_cost: None,
            },
        )
    }

    fn solve<'a>(
        &'a self,
        request: &'a SolveRequest,
        output: &'a mut dyn BackendOutputSink,
    ) -> BackendSolveFuture<'a> {
        Box::pin(async move {
            self.invocations.fetch_add(1, Ordering::SeqCst);
            let mut values = CandidateValues::default();
            for variable in &request.problem().variables {
                match variable {
                    Variable::Boolean(variable) => {
                        values.booleans.insert(variable.id.clone(), true);
                    }
                    Variable::Integer(variable) => {
                        values.integers.insert(variable.id.clone(), 3);
                    }
                    Variable::Interval(_) => {
                        return Err(BackendError::new(
                            "tests.phase02.unexpected_interval",
                            "the trivial official fixture unexpectedly contained an interval",
                        )?);
                    }
                }
            }
            let raw_objective = BackendObjectiveEvidence {
                objective_values: vec![RAW_BACKEND_OBJECTIVE],
                best_bound_values: Some(vec![RAW_BACKEND_OBJECTIVE]),
            };
            output.submit_candidate(CandidateSubmission {
                values,
                observed_after_milliseconds: duration(1)?,
                objective: Some(raw_objective.clone()),
                evidence_refs: Vec::new(),
            })?;
            Ok(BackendSolveOutcome {
                backend_id: self.descriptor.id.clone(),
                model_hash: request.model_hash().to_owned(),
                solve_fingerprint: request.solve_fingerprint().to_owned(),
                termination: BackendTerminationReason::CandidateFound,
                evidence: BackendTerminationEvidence {
                    remaining_at_dispatch_milliseconds: request
                        .dispatch_budget()
                        .remaining_at_dispatch(),
                    backend_limit_milliseconds: request.dispatch_budget().backend_limit(),
                    elapsed_milliseconds: duration(1)?,
                    first_incumbent_milliseconds: Some(duration(1)?),
                    objective: Some(raw_objective),
                    evidence_refs: Vec::new(),
                },
            })
        })
    }
}

struct ReviewingCandidate<'a> {
    pack: &'a dyn DomainPack,
    problem: &'a PlanningProblem,
    document: &'a ScenarioDocument,
    solution_id: SolutionId,
    solution: Option<NormalizedSolution>,
    verification: Option<VerificationReport>,
    score: Option<ScoreVector>,
    observed_raw_objective: Option<i64>,
}

impl CandidateReviewer for ReviewingCandidate<'_> {
    fn review(&mut self, _backend_id: &BackendId, candidate: &BackendCandidate) -> CandidateReview {
        self.observed_raw_objective = candidate
            .objective
            .as_ref()
            .and_then(|objective| objective.objective_values.first().copied());
        let Ok(solution) = self
            .pack
            .project(self.problem, &candidate.values, self.solution_id)
        else {
            return CandidateReview::VerificationFailed {
                diagnostic_code: "verification.projection_failed".to_owned(),
            };
        };
        let Ok(verification) = self.pack.verify(self.document, &solution) else {
            return CandidateReview::VerificationFailed {
                diagnostic_code: "verification.failed".to_owned(),
            };
        };
        let Ok(score) = self.pack.score(self.document, &solution) else {
            return CandidateReview::VerificationFailed {
                diagnostic_code: "verification.score_failed".to_owned(),
            };
        };
        if !verification.feasible || verification.score.as_ref() != Some(&score) {
            return CandidateReview::VerificationFailed {
                diagnostic_code: "verification.rejected".to_owned(),
            };
        }
        self.solution = Some(solution);
        self.verification = Some(verification);
        self.score = Some(score);
        CandidateReview::Verified
    }
}

struct Progress;

impl ProgressSink for Progress {
    fn emit(&mut self, _event: SolveProgressEvent) -> Result<(), OutputError> {
        Ok(())
    }
}

fn document() -> Result<ScenarioDocument, serde_json::Error> {
    serde_json::from_value(json!({
        "format": "eutheto/scenario",
        "formatVersion": 1,
        "scenarioId": SCENARIO_ID,
        "domainPack": { "id": OFFICIAL_TEST_PACK_ID, "schemaVersion": 1 },
        "metadata": {
            "title": "Phase-02 routed vertical",
            "description": "",
            "createdAt": "2026-08-29T12:00:00Z",
            "updatedAt": "2026-08-29T12:00:00Z"
        },
        "settings": {
            "timeZone": "UTC",
            "locale": "en-US",
            "units": "metric",
            "horizon": {
                "start": "2026-08-29T12:00:00Z",
                "end": "2026-09-05T12:00:00Z"
            },
            "gapPolicy": "reject",
            "overlapPolicy": "earlier"
        },
        "domain": {
            "entities": {
                (ENTITY_ID): { "id": ENTITY_ID, "enabled": true, "target": 3 }
            },
            "rules": {},
            "preferences": {},
            "lockedAssignments": {}
        },
        "extensions": {}
    }))
}

fn compile_context() -> CompileContext {
    CompileContext {
        scenario_revision: 12,
        semantic_metadata: BTreeMap::new(),
        cancellation: CancellationToken::new(),
        planning_limits: PlanningIrLimitsV1::DEFAULT,
    }
}

fn solve_options(backend_id: &str) -> Result<SolveOptions, Box<dyn Error>> {
    Ok(SolveOptions {
        backend: BackendSelection::Specific(BackendId::new(backend_id)?),
        mode: SolveMode::Custom,
        time_limit_milliseconds: DurationMillis::new(1_000)?,
        memory_limit_bytes: Some(1024 * 1024),
        worker_threads: WorkerThreadPolicy::Exact(1),
        random_seed: 7,
        solution_limit: Some(1),
        stop_after_first_feasible: true,
        collect_intermediate_solutions: false,
        explanation_mode: ExplanationMode::None,
        preserve_existing: PreservationPolicy::None,
        reproducibility: ReproducibilityMode::Deterministic,
        resource_limits: ResourceLimits {
            max_entities: 100,
            max_rules: 100,
            max_variables: 100,
            max_constraints: 100,
        },
    })
}

fn support_features() -> Result<Vec<SupportFeature>, SupportMatrixError> {
    SUPPORT_FEATURES
        .iter()
        .map(|(id, category, gate)| {
            Ok(SupportFeature {
                id: SupportFeatureId::new(*id)?,
                category: match *category {
                    "primitive" => SupportFeatureCategory::Primitive,
                    "objective" => SupportFeatureCategory::Objective,
                    "projection" => SupportFeatureCategory::Projection,
                    "solve" => SupportFeatureCategory::Solve,
                    value => {
                        return Err(SupportMatrixError::UnknownGeneratedCategory(
                            value.to_owned(),
                        ));
                    }
                },
                gate: if *gate == "unconditional" {
                    SupportFeatureGate::Unconditional
                } else {
                    SupportFeatureGate::Enabled((*gate).to_owned())
                },
            })
        })
        .collect()
}

fn test_registry(
    backend_id: &str,
    unsupported_feature: Option<&str>,
) -> Result<(SolverRegistry, Arc<ExactTestBackend>), Box<dyn Error>> {
    let backend_id = BackendId::new(backend_id)?;
    let features = support_features()?;
    let mut supported = BTreeSet::new();
    let cells = features
        .iter()
        .map(|feature| {
            let cell = if unsupported_feature == Some(feature.id.as_str()) {
                SupportCell::Unsupported {
                    reason: UNSUPPORTED_REASON.to_owned(),
                    remediation: UNSUPPORTED_REMEDIATION.to_owned(),
                    fixture_id: "tests.phase02.unsupported-objective-penalty".to_owned(),
                }
            } else {
                supported.insert(feature.id.clone());
                SupportCell::Supported {
                    fixture_id: format!("tests.phase02.{}", feature.id.as_str().replace('.', "-")),
                }
            };
            (feature.id.clone(), cell)
        })
        .collect();
    let descriptor = SolverDescriptor {
        id: backend_id.clone(),
        display_name: "Phase-02 exact test backend".to_owned(),
        version: "1.0.0".to_owned(),
        adapter_version: "1.0.0".to_owned(),
        distribution: SolverDistribution::BuiltIn,
        license: LicenseMetadata {
            spdx_expression: "Apache-2.0".to_owned(),
            license_name: "Apache License 2.0".to_owned(),
            source_url: None,
        },
        stability: BackendStability::Experimental,
        capabilities: SolverCapabilities {
            supported,
            degraded: BTreeSet::new(),
        },
    };
    let matrix = CapabilityMatrix::new(
        SUPPORT_MATRIX_SCHEMA_VERSION,
        SUPPORT_MATRIX_IR_SCHEMA_VERSION,
        features,
        vec![BackendSupportColumn {
            backend_id,
            backend_version: descriptor.version.clone(),
            adapter_version: descriptor.adapter_version.clone(),
            cells,
        }],
        Vec::new(),
    )?;
    let backend = Arc::new(ExactTestBackend {
        descriptor,
        matrix: matrix.clone(),
        invocations: AtomicU32::new(0),
    });
    let registered: Vec<Arc<dyn SolverBackend>> = vec![backend.clone()];
    Ok((SolverRegistry::new(matrix, registered)?, backend))
}

fn duration(value: u64) -> Result<DurationMillis, BackendError> {
    DurationMillis::new(value).map_err(|_| BackendError::from(OutputError::UnsafeDiagnosticLine))
}

#[tokio::test]
async fn official_pack_candidate_is_projected_verified_and_scored_before_router_acceptance()
-> TestResult {
    let domain_registry = official_registry()?;
    let pack = domain_registry.require(&PackId::new(OFFICIAL_TEST_PACK_ID)?)?;
    let source = document()?;
    let context = compile_context();
    let problem = pack.compile(&source, &context)?;
    validate(&problem, context.planning_limits)?;
    let summary = summarize(&problem, context.planning_limits)?;
    assert_eq!(
        canonical_ir_hash(&problem, context.planning_limits)?,
        summary.canonical_ir_hash
    );
    assert_eq!(summary.bool_variable_count, 1);
    assert_eq!(summary.int_variable_count, 1);
    assert_eq!(summary.objective_term_count, 1);
    assert_eq!(summary.projection_count, 2);
    let problem = Arc::new(problem);

    let (solver_registry, backend) = test_registry(EXACT_BACKEND_ID, None)?;
    let options = solve_options(EXACT_BACKEND_ID)?;
    assert!(backend.compatibility(&summary, &options).compatible());
    let clock = FixedMonotonicClock::default();
    let parent = ParentSolveBudget::new(
        DurationMillis::new(1_000)?,
        Arc::new(clock),
        CancellationToken::new(),
    )?;
    let mut progress = Progress;
    let mut reviewer = ReviewingCandidate {
        pack,
        problem: problem.as_ref(),
        document: &source,
        solution_id: SolutionId::from_str(SOLUTION_ID)?,
        solution: None,
        verification: None,
        score: None,
        observed_raw_objective: None,
    };
    let execution = SolverRouter::new(&solver_registry)
        .execute(
            problem.clone(),
            options,
            &parent,
            &mut progress,
            &mut reviewer,
        )
        .await;

    assert_eq!(
        execution.terminal_reason,
        ExecutionTerminalReason::CandidateVerified
    );
    assert_eq!(execution.terminal_status, SolveStatus::Feasible);
    assert_eq!(execution.invocation_count, 1);
    assert_eq!(backend.invocations.load(Ordering::SeqCst), 1);
    assert_eq!(reviewer.observed_raw_objective, Some(RAW_BACKEND_OBJECTIVE));
    assert_eq!(
        execution
            .selected_candidate
            .as_ref()
            .and_then(|candidate| candidate.objective.as_ref())
            .and_then(|objective| objective.objective_values.first()),
        Some(&RAW_BACKEND_OBJECTIVE)
    );

    let solution = reviewer
        .solution
        .as_ref()
        .ok_or("verified solution missing")?;
    let enabled_assignment = format!("official.test.assignment.enabled.{ENTITY_ID}");
    let target_assignment = format!("official.test.assignment.target.{ENTITY_ID}");
    assert_eq!(
        solution
            .assignments
            .iter()
            .find(|assignment| assignment.id.as_str() == enabled_assignment.as_str())
            .map(|assignment| &assignment.value),
        Some(&AssignmentValue::Boolean(true))
    );
    assert_eq!(
        solution
            .assignments
            .iter()
            .find(|assignment| assignment.id.as_str() == target_assignment.as_str())
            .map(|assignment| &assignment.value),
        Some(&AssignmentValue::Integer(3))
    );
    let score = reviewer
        .score
        .as_ref()
        .ok_or("authoritative score missing")?;
    assert_eq!(score.feasibility, 0);
    assert_eq!(score.levels[0].value, 3);
    assert_ne!(score.levels[0].value, RAW_BACKEND_OBJECTIVE);
    assert_eq!(
        reviewer
            .verification
            .as_ref()
            .and_then(|report| report.score.as_ref()),
        Some(score)
    );
    Ok(())
}

#[tokio::test]
async fn incompatible_backend_reports_exact_gap_and_is_never_invoked() -> TestResult {
    let domain_registry = official_registry()?;
    let pack = domain_registry.require(&PackId::new(OFFICIAL_TEST_PACK_ID)?)?;
    let context = compile_context();
    let problem = pack.compile(&document()?, &context)?;
    let summary = summarize(&problem, context.planning_limits)?;
    let (solver_registry, backend) =
        test_registry(INCOMPATIBLE_BACKEND_ID, Some(UNSUPPORTED_FEATURE_ID))?;
    let options = solve_options(INCOMPATIBLE_BACKEND_ID)?;
    let compatibility = backend.compatibility(&summary, &options);
    assert_eq!(compatibility.level, CompatibilityLevel::Unsupported);
    assert_eq!(compatibility.unsupported_features.len(), 1);
    let unsupported = &compatibility.unsupported_features[0];
    assert_eq!(unsupported.feature_id.as_str(), UNSUPPORTED_FEATURE_ID);
    assert_eq!(unsupported.usage_count, 1);
    assert_eq!(
        unsupported.path,
        "planningProblem.manifest.capabilityCounts"
    );
    assert_eq!(unsupported.reason, UNSUPPORTED_REASON);
    assert_eq!(unsupported.remediation, UNSUPPORTED_REMEDIATION);

    let clock = FixedMonotonicClock::default();
    let parent = ParentSolveBudget::new(
        DurationMillis::new(1_000)?,
        Arc::new(clock),
        CancellationToken::new(),
    )?;
    let mut progress = Progress;
    let mut reviewer = RequireIndependentVerification;
    let execution = SolverRouter::new(&solver_registry)
        .execute(
            Arc::new(problem),
            options,
            &parent,
            &mut progress,
            &mut reviewer,
        )
        .await;
    assert_eq!(execution.invocation_count, 0);
    assert_eq!(backend.invocations.load(Ordering::SeqCst), 0);
    assert_eq!(
        execution.terminal_reason,
        ExecutionTerminalReason::UnsupportedOverride
    );
    assert!(execution.selected_candidate.is_none());
    Ok(())
}
