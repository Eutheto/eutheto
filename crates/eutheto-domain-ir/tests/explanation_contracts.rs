use eutheto_domain_ir::*;
use eutheto_types::{
    BackendId, BackendSelection, CounterfactualJobId, DurationMillis, ExplanationMode,
    IanaTimeZone, PackId, PreservationPolicy, ReproducibilityMode, RequestId, ResourceLimits,
    Rfc3339Timestamp, ScenarioId, ScenarioSnapshotId, SolutionId, SolveMode, SolveOptions,
    SolveRunId, SolveStatus, WorkerThreadPolicy,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::error::Error;

fn uuid(suffix: u8) -> String {
    format!("01890a5d-ac96-7b64-9f74-bbfcf30f9f{suffix:02x}")
}

fn hash(label: &str) -> String {
    blake3_hex(label.as_bytes())
}

fn result_ref(suffix: u8) -> Result<AcceptedResultRefV1, Box<dyn Error>> {
    Ok(AcceptedResultRefV1 {
        solution_id: uuid(suffix).parse::<SolutionId>()?,
        result_checksum: hash(&format!("result-{suffix}")),
    })
}

fn score(value: i64) -> Result<ScoreVector, Box<dyn Error>> {
    Ok(ScoreVector {
        feasibility: 0,
        levels: vec![ScoreLevelValue {
            level_id: ScoreLevelId::new("score.primary")?,
            value,
            direction: OptimizationDirection::Minimize,
            category_breakdown: BTreeMap::new(),
        }],
    })
}

fn comparison() -> Result<SolutionComparisonV1, Box<dyn Error>> {
    let base = result_ref(1)?;
    let candidate = result_ref(2)?;
    let pack_id = PackId::new("official.synthetic")?;
    let scenario_id = uuid(0x80).parse::<ScenarioId>()?;
    let side = |revision, document: &str, accepted_result| ComparisonBindingV1 {
        pack_id: pack_id.clone(),
        scenario_id,
        scenario_revision: revision,
        document_hash: hash(document),
        projection_version: 1,
        verification_scope_checksum: hash(&format!("scope-{revision}")),
        accepted_result,
    };
    Ok(SolutionComparisonV1::new(
        side(7, "base-document", base),
        side(8, "candidate-document", candidate),
        score(4)?,
        score(4)?,
        Vec::new(),
        Vec::new(),
        vec![ScoreLevelComparisonV1 {
            level_id: ScoreLevelId::new("score.primary")?,
            direction: OptimizationDirection::Minimize,
            before: 4,
            after: 4,
            delta: 0,
            categories: Vec::new(),
        }],
        Vec::new(),
        Vec::new(),
        Some(RunComparisonV1 {
            base: RunComparisonSideV1 {
                run_id: uuid(3).parse()?,
                run_manifest_checksum: hash("base-manifest"),
                outcome: RunTerminalOutcomeV1::NoResult {
                    status: SolveStatus::InvalidModel,
                },
                certainty: ExplanationCertainty::Deterministic,
            },
            candidate: RunComparisonSideV1 {
                run_id: uuid(4).parse()?,
                run_manifest_checksum: hash("candidate-manifest"),
                outcome: RunTerminalOutcomeV1::NoResult {
                    status: SolveStatus::InvalidModel,
                },
                certainty: ExplanationCertainty::Deterministic,
            },
        }),
        Vec::new(),
        ComparisonOrdering::Equivalent,
    )?)
}

fn solve_options() -> Result<SolveOptions, Box<dyn Error>> {
    Ok(SolveOptions {
        backend: BackendSelection::Auto,
        mode: SolveMode::Balanced,
        time_limit_milliseconds: DurationMillis::new(5_000)?,
        memory_limit_bytes: Some(64 * 1024 * 1024),
        worker_threads: WorkerThreadPolicy::Exact(1),
        random_seed: 9,
        solution_limit: Some(1),
        stop_after_first_feasible: true,
        collect_intermediate_solutions: false,
        explanation_mode: ExplanationMode::Standard,
        preserve_existing: PreservationPolicy::None,
        reproducibility: ReproducibilityMode::Deterministic,
        resource_limits: ResourceLimits {
            max_entities: 100,
            max_rules: 100,
            max_variables: 1_000,
            max_constraints: 1_000,
        },
    })
}

fn rebuild_run_input(
    input: &RunInputV1,
    run_id: SolveRunId,
    compiler_version: String,
) -> Result<RunInputV1, DomainContractError> {
    RunInputV1::new(
        run_id,
        input.request_id,
        input.scenario_id,
        input.scenario_revision,
        input.snapshot_id,
        input.snapshot_document_hash.clone(),
        input.snapshot_created_at,
        input.pack_id.clone(),
        input.pack_schema_version,
        input.planning_ir_schema_version,
        compiler_version,
        input.application_version.clone(),
        input.backend_id.clone(),
        input.backend_version.clone(),
        input.adapter_version.clone(),
        input.worker_version.clone(),
        input.solver_version.clone(),
        input.protocol_major,
        input.protocol_minor,
        input.model_hash.clone(),
        input.objective_policy_hash.clone(),
        input.solve_options.clone(),
        input.scenario_timezone.clone(),
        input.temporary_condition_hash.clone(),
    )
}

fn infeasible_manifest(input: &RunInputV1) -> Result<RunManifestV1, Box<dyn Error>> {
    Ok(RunManifestV1::new(
        input.run_id,
        input.checksum.clone(),
        RunTerminalOutcomeV1::NoResult {
            status: SolveStatus::Infeasible,
        },
        "2026-09-03T12:00:01Z".parse()?,
        "2026-09-03T12:00:02Z".parse()?,
        Some(DurationMillis::new(1_000)?),
        None,
        None,
        RunPhaseTimingsV1::default(),
        Vec::new(),
    )?)
}

struct CounterfactualFixture {
    result: CounterfactualResultV1,
    request: CounterfactualJobRequestV1,
}

#[allow(clippy::too_many_lines)]
fn counterfactual_fixture() -> Result<CounterfactualFixture, Box<dyn Error>> {
    let condition =
        CounterfactualConditionV1::new(CounterfactualConditionPayloadV1::ForceAssignmentValue {
            assignment_id: DomainAssignmentId::new("assignment.target")?,
            value: AssignmentValue::Boolean(true),
        })?;
    let base_model_hash = hash("base-model");
    let derived_model_hash = hash("derived-model");
    let objective_policy_hash = hash("objective-policy");
    let compilation = CounterfactualCompilationBindingV1::new(
        base_model_hash.clone(),
        condition.checksum.clone(),
        derived_model_hash.clone(),
        objective_policy_hash.clone(),
    )?;
    let scenario_id = uuid(0x80).parse::<ScenarioId>()?;
    let snapshot_id = uuid(0x11).parse::<ScenarioSnapshotId>()?;
    let snapshot_document_hash = hash("snapshot");
    let pack_id = PackId::new("official.synthetic")?;
    let base_run_id = uuid(0x14).parse::<SolveRunId>()?;
    let base_run_input = RunInputV1::new(
        base_run_id,
        uuid(0x16).parse::<RequestId>()?,
        scenario_id,
        7,
        snapshot_id,
        snapshot_document_hash.clone(),
        "2026-09-03T11:59:00Z".parse::<Rfc3339Timestamp>()?,
        pack_id.clone(),
        1,
        2,
        "1.0.0".to_owned(),
        "1.0.0".to_owned(),
        BackendId::new("ortools.cp-sat")?,
        "9.15.6755".to_owned(),
        "1.0.0".to_owned(),
        "1.0.0".to_owned(),
        "9.15.6755".to_owned(),
        1,
        0,
        base_model_hash.clone(),
        objective_policy_hash.clone(),
        solve_options()?,
        "UTC".parse::<IanaTimeZone>()?,
        None,
    )?;
    let base = result_ref(1)?;
    let base_run_manifest = RunManifestV1::new(
        base_run_id,
        base_run_input.checksum.clone(),
        RunTerminalOutcomeV1::Accepted {
            status: SolveStatus::Feasible,
            solution_id: base.solution_id,
            accepted_result_checksum: base.result_checksum.clone(),
            verification_checksum: hash("base-verification"),
        },
        "2026-09-03T11:59:01Z".parse()?,
        "2026-09-03T11:59:02Z".parse()?,
        Some(DurationMillis::new(1_000)?),
        Some(DurationMillis::new(500)?),
        Some(DurationMillis::new(800)?),
        RunPhaseTimingsV1::default(),
        Vec::new(),
    )?;
    let run_id = uuid(0x10).parse::<SolveRunId>()?;
    let run_input = RunInputV1::new(
        run_id,
        uuid(0x12).parse::<RequestId>()?,
        scenario_id,
        7,
        snapshot_id,
        snapshot_document_hash.clone(),
        "2026-09-03T12:00:00Z".parse::<Rfc3339Timestamp>()?,
        pack_id,
        1,
        2,
        "1.0.0".to_owned(),
        "1.0.0".to_owned(),
        BackendId::new("ortools.cp-sat")?,
        "9.15.6755".to_owned(),
        "1.0.0".to_owned(),
        "1.0.0".to_owned(),
        "9.15.6755".to_owned(),
        1,
        0,
        derived_model_hash,
        objective_policy_hash.clone(),
        solve_options()?,
        "UTC".parse::<IanaTimeZone>()?,
        Some(condition.checksum.clone()),
    )?;
    let run_manifest = RunManifestV1::new(
        run_id,
        run_input.checksum.clone(),
        RunTerminalOutcomeV1::NoResult {
            status: SolveStatus::Infeasible,
        },
        "2026-09-03T12:00:01Z".parse()?,
        "2026-09-03T12:00:02Z".parse()?,
        Some(DurationMillis::new(1_000)?),
        None,
        None,
        RunPhaseTimingsV1::default(),
        Vec::new(),
    )?;
    let job_id = uuid(0x13).parse::<CounterfactualJobId>()?;
    let semantics = CounterfactualRequestSemanticsV1 {
        schema_version: COUNTERFACTUAL_REQUEST_SEMANTICS_SCHEMA_VERSION,
        scenario_id,
        scenario_revision: 7,
        snapshot_id,
        snapshot_document_hash,
        base,
        base_run_id,
        base_run_input_checksum: base_run_input.checksum.clone(),
        base_model_hash,
        objective_policy_hash,
        condition_checksum: condition.checksum.clone(),
        total_budget_milliseconds: DurationMillis::new(5_000)?,
    };
    let request = CounterfactualJobRequestV1::new(
        job_id,
        uuid(0x15).parse()?,
        semantics,
        condition,
        "2026-09-03T12:00:00Z".parse()?,
    )?;
    let result = CounterfactualResultV1::new(
        request.clone(),
        base_run_input,
        base_run_manifest,
        compilation,
        run_input,
        run_manifest,
        CounterfactualConclusionV1::ProvenImpossible,
    )?;
    Ok(CounterfactualFixture { result, request })
}

fn counterfactual_comparison(
    result: &CounterfactualResultV1,
    base: AcceptedResultRefV1,
    candidate: AcceptedResultRefV1,
) -> Result<SolutionComparisonV1, Box<dyn Error>> {
    let scope = hash("counterfactual-scope");
    let binding = |accepted_result| ComparisonBindingV1 {
        pack_id: result.run_input.pack_id.clone(),
        scenario_id: result.request.semantics.scenario_id,
        scenario_revision: result.request.semantics.scenario_revision,
        document_hash: result.request.semantics.snapshot_document_hash.clone(),
        projection_version: 1,
        verification_scope_checksum: scope.clone(),
        accepted_result,
    };
    Ok(SolutionComparisonV1::new(
        binding(base),
        binding(candidate),
        score(4)?,
        score(4)?,
        Vec::new(),
        Vec::new(),
        vec![ScoreLevelComparisonV1 {
            level_id: ScoreLevelId::new("score.primary")?,
            direction: OptimizationDirection::Minimize,
            before: 4,
            after: 4,
            delta: 0,
            categories: Vec::new(),
        }],
        Vec::new(),
        Vec::new(),
        None,
        Vec::new(),
        ComparisonOrdering::Equivalent,
    )?)
}

fn validation_issue(
    severity: ExplanationValidationSeverity,
) -> Result<ValidationIssueEvidenceV1, Box<dyn Error>> {
    Ok(ValidationIssueEvidenceV1 {
        issue_id: VerificationIssueId::new("validation.issue")?,
        severity,
        message_key: "validation.issue.message".to_owned(),
        parameters: BTreeMap::from([(
            VerificationFactId::new("fact.count")?,
            VerificationValue::Integer(2),
        )]),
        field_path: Some(vec![VerificationFactId::new("field.assignments")?]),
        entity: None,
        rule_id: None,
    })
}

fn assignment_evidence() -> Result<AssignmentEvidenceV1, Box<dyn Error>> {
    Ok(AssignmentEvidenceV1 {
        assignment: DomainAssignment {
            id: DomainAssignmentId::new("assignment.target")?,
            entity: DomainEntityRef {
                kind: DomainEntityKindId::new("entity.person")?,
                id: DomainEntityId::new("person.alice")?,
            },
            value: AssignmentValue::Boolean(true),
            evidence: Vec::new(),
        },
        related_rules: Vec::new(),
        score_contributions: vec![ScoreContributionV1 {
            evidence_id: DomainEvidenceId::new("contribution.primary")?,
            level_id: ScoreLevelId::new("score.primary")?,
            category_id: None,
            value: 4,
        }],
        metrics: BTreeMap::new(),
        lock_state: Some(AssignmentLockStateV1::Locked {
            value: AssignmentValue::Boolean(true),
        }),
    })
}

fn all_evidence() -> Result<Vec<ExplanationEvidenceV1>, Box<dyn Error>> {
    let counterfactual = counterfactual_fixture()?.result;
    let optimality_status = OptimalityStatusEvidenceV1 {
        run_input: counterfactual.run_input.clone(),
        run_manifest: counterfactual.run_manifest.clone(),
        result: None,
    };
    let comparison = comparison()?;
    Ok(vec![
        ExplanationEvidenceV1::new(ExplanationEvidencePayloadV1::Validation {
            issue: validation_issue(ExplanationValidationSeverity::MustFix)?,
        })?,
        ExplanationEvidenceV1::new(ExplanationEvidencePayloadV1::Infeasibility {
            infeasibility: InfeasibilityEvidenceV1::Unavailable {
                reason: ConflictUnavailableReason::ConflictNotReturned,
            },
        })?,
        ExplanationEvidenceV1::new(ExplanationEvidencePayloadV1::Assignment {
            assignment: assignment_evidence()?,
        })?,
        ExplanationEvidenceV1::new(ExplanationEvidencePayloadV1::Counterfactual {
            result: Box::new(counterfactual),
        })?,
        ExplanationEvidenceV1::new(ExplanationEvidencePayloadV1::SolutionDifference {
            comparison: Box::new(comparison.clone()),
        })?,
        ExplanationEvidenceV1::new(ExplanationEvidencePayloadV1::Repair {
            repair: Box::new(RepairEvidenceV1 {
                comparison,
                causality: RepairCausalityV1::NotEstablished,
            }),
        })?,
        ExplanationEvidenceV1::new(ExplanationEvidencePayloadV1::OptimalityStatus {
            status: Box::new(optimality_status),
        })?,
    ])
}

#[test]
fn exact_seven_request_subjects_round_trip_strictly() -> Result<(), Box<dyn Error>> {
    let accepted = result_ref(1)?;
    let subjects = vec![
        ExplanationRequestSubjectV1::Validation { issue_id: None },
        ExplanationRequestSubjectV1::Infeasibility {
            solve_run_id: uuid(1).parse()?,
            run_manifest_checksum: hash("manifest"),
            conflict_id: Some(DomainEvidenceId::new("conflict.retained")?),
        },
        ExplanationRequestSubjectV1::Assignment {
            result: accepted.clone(),
            assignment_id: DomainAssignmentId::new("assignment.target")?,
        },
        ExplanationRequestSubjectV1::Counterfactual {
            job_id: uuid(2).parse()?,
            base: accepted.clone(),
        },
        ExplanationRequestSubjectV1::SolutionDifference {
            left: accepted.clone(),
            right: result_ref(2)?,
        },
        ExplanationRequestSubjectV1::Repair {
            current: result_ref(2)?,
            base: accepted.clone(),
        },
        ExplanationRequestSubjectV1::OptimalityStatus {
            solve_run_id: uuid(3).parse()?,
            run_manifest_checksum: hash("status-manifest"),
            result: Some(accepted),
        },
    ];
    let expected = [
        ExplanationKind::Validation,
        ExplanationKind::Infeasibility,
        ExplanationKind::Assignment,
        ExplanationKind::Counterfactual,
        ExplanationKind::SolutionDifference,
        ExplanationKind::Repair,
        ExplanationKind::OptimalityStatus,
    ];
    assert_eq!(subjects.len(), expected.len());
    for (subject, kind) in subjects.into_iter().zip(expected) {
        let request = ExplanationRequestV1::new(subject)?;
        assert_eq!(request.subject.kind(), kind);
        let bytes = serde_json::to_vec(&request)?;
        assert_eq!(ExplanationRequestV1::from_json(&bytes)?, request);
    }
    Ok(())
}

#[test]
fn exact_seven_evidence_and_result_discriminants_round_trip() -> Result<(), Box<dyn Error>> {
    let expected = [
        ExplanationKind::Validation,
        ExplanationKind::Infeasibility,
        ExplanationKind::Assignment,
        ExplanationKind::Counterfactual,
        ExplanationKind::SolutionDifference,
        ExplanationKind::Repair,
        ExplanationKind::OptimalityStatus,
    ];
    for (evidence, kind) in all_evidence()?.into_iter().zip(expected) {
        assert_eq!(evidence.kind(), kind);
        assert_eq!(
            ExplanationEvidenceV1::from_json(&serde_json::to_vec(&evidence)?)?,
            evidence
        );
        let request = EvidenceRenderRequestV1::new(evidence.clone())?;
        let rendered = EvidenceRenderResultV1::new(
            &request,
            vec![EvidenceMessageV1 {
                message_key: "evidence.summary".to_owned(),
                parameters: BTreeMap::new(),
                entities: Vec::new(),
                rules: Vec::new(),
                assignments: Vec::new(),
                evidence: Vec::new(),
            }],
        )?;
        let result = ExplanationResultV1::new(evidence, rendered)?;
        assert_eq!(
            ExplanationResultV1::from_json(&serde_json::to_vec(&result)?)?,
            result
        );
    }
    Ok(())
}

#[test]
fn render_result_rejects_same_kind_evidence_substitution() -> Result<(), Box<dyn Error>> {
    let first = ExplanationEvidenceV1::new(ExplanationEvidencePayloadV1::Validation {
        issue: validation_issue(ExplanationValidationSeverity::MustFix)?,
    })?;
    let second = ExplanationEvidenceV1::new(ExplanationEvidencePayloadV1::Validation {
        issue: validation_issue(ExplanationValidationSeverity::LikelyProblem)?,
    })?;
    let request = EvidenceRenderRequestV1::new(first)?;
    let rendered = EvidenceRenderResultV1::new(
        &request,
        vec![EvidenceMessageV1 {
            message_key: "evidence.summary".to_owned(),
            parameters: BTreeMap::new(),
            entities: Vec::new(),
            rules: Vec::new(),
            assignments: Vec::new(),
            evidence: Vec::new(),
        }],
    )?;
    assert!(matches!(
        ExplanationResultV1::new(second, rendered),
        Err(DomainContractError::InvalidExplanationContract(
            "explanation result evidence binding"
        ))
    ));
    Ok(())
}

#[test]
fn optimality_status_rejects_detached_run_authority() -> Result<(), Box<dyn Error>> {
    let fixture = counterfactual_fixture()?;
    let status = OptimalityStatusEvidenceV1 {
        run_input: fixture.result.base_run_input,
        run_manifest: fixture.result.run_manifest,
        result: None,
    };
    assert!(matches!(
        status.validate(),
        Err(DomainContractError::InvalidExplanationContract(
            "optimality run binding"
        ))
    ));
    Ok(())
}

#[test]
fn explanation_ingress_rejects_unknowns_and_certainty_kind_checksum_tampering()
-> Result<(), Box<dyn Error>> {
    let evidence = all_evidence()?.remove(0);
    let mut value = serde_json::to_value(&evidence)?;
    value["unknown"] = json!(true);
    assert!(ExplanationEvidenceV1::from_json(&serde_json::to_vec(&value)?).is_err());

    let mut value = serde_json::to_value(&evidence)?;
    value["schemaVersion"] = json!(2);
    assert!(matches!(
        ExplanationEvidenceV1::from_json(&serde_json::to_vec(&value)?),
        Err(DomainContractError::UnsupportedVersion(2))
    ));

    let mut value = serde_json::to_value(&evidence)?;
    value["certainty"] = json!("backendProof");
    assert!(matches!(
        ExplanationEvidenceV1::from_json(&serde_json::to_vec(&value)?),
        Err(DomainContractError::InvalidExplanationContract(
            "evidence certainty"
        ))
    ));

    let request = EvidenceRenderRequestV1::new(evidence.clone())?;
    let mut value = serde_json::to_value(&request)?;
    value["kind"] = json!("assignment");
    assert!(matches!(
        EvidenceRenderRequestV1::from_json(&serde_json::to_vec(&value)?),
        Err(DomainContractError::InvalidExplanationContract(
            "render request kind"
        ))
    ));

    let mut value = serde_json::to_value(&evidence)?;
    value["checksum"] = json!(hash("tampered"));
    assert!(matches!(
        ExplanationEvidenceV1::from_json(&serde_json::to_vec(&value)?),
        Err(DomainContractError::ChecksumMismatch)
    ));
    Ok(())
}

#[test]
fn evidence_references_are_canonical_bounded_and_text_is_safe() -> Result<(), Box<dyn Error>> {
    let mut message = EvidenceMessageV1 {
        message_key: "evidence.message".to_owned(),
        parameters: BTreeMap::new(),
        entities: Vec::new(),
        rules: Vec::new(),
        assignments: vec![
            DomainAssignmentId::new("assignment.second")?,
            DomainAssignmentId::new("assignment.first")?,
        ],
        evidence: Vec::new(),
    };
    assert!(matches!(
        message.validate(),
        Err(DomainContractError::NonCanonicalExplanationCollection(
            "message assignments"
        ))
    ));
    message.assignments = (0..=MAX_EVIDENCE_MESSAGE_REFERENCES)
        .map(|index| DomainAssignmentId::new(format!("assignment.a{index:04}")))
        .collect::<Result<_, _>>()?;
    assert!(matches!(
        message.validate(),
        Err(DomainContractError::LimitExceeded("message assignments"))
    ));
    message.assignments.clear();
    message.parameters.insert(
        VerificationFactId::new("fact.unsafe")?,
        VerificationValue::Text(String::new()),
    );
    assert!(matches!(
        message.validate(),
        Err(DomainContractError::InvalidVerificationText)
    ));

    let mut assignment = assignment_evidence()?;
    let mut duplicate = assignment.score_contributions[0].clone();
    duplicate.value = 5;
    assignment.score_contributions.push(duplicate);
    assert!(matches!(
        assignment.validate(),
        Err(DomainContractError::NonCanonicalExplanationCollection(
            "score contributions"
        ))
    ));
    Ok(())
}

#[test]
fn all_four_validation_severities_are_distinct_and_strict() -> Result<(), Box<dyn Error>> {
    for severity in [
        ExplanationValidationSeverity::MustFix,
        ExplanationValidationSeverity::LikelyProblem,
        ExplanationValidationSeverity::ReviewSuggested,
        ExplanationValidationSeverity::Information,
    ] {
        let issue = validation_issue(severity)?;
        issue.validate()?;
        assert_eq!(
            serde_json::from_slice::<ValidationIssueEvidenceV1>(&serde_json::to_vec(&issue)?)?,
            issue
        );
    }
    assert!(serde_json::from_value::<ExplanationValidationSeverity>(json!("warning")).is_err());
    Ok(())
}

#[test]
fn force_and_forbid_are_the_only_checksummed_conditions() -> Result<(), Box<dyn Error>> {
    for condition in [
        CounterfactualConditionPayloadV1::ForceAssignmentValue {
            assignment_id: DomainAssignmentId::new("assignment.target")?,
            value: AssignmentValue::Integer(4),
        },
        CounterfactualConditionPayloadV1::ForbidAssignmentValue {
            assignment_id: DomainAssignmentId::new("assignment.target")?,
            value: AssignmentValue::Integer(4),
        },
    ] {
        let condition = CounterfactualConditionV1::new(condition)?;
        assert_eq!(
            CounterfactualConditionV1::from_json(&serde_json::to_vec(&condition)?)?,
            condition
        );
        let mut tampered = serde_json::to_value(&condition)?;
        tampered["checksum"] = json!(hash("wrong"));
        assert!(matches!(
            CounterfactualConditionV1::from_json(&serde_json::to_vec(&tampered)?),
            Err(DomainContractError::ChecksumMismatch)
        ));
    }
    assert!(
        serde_json::from_value::<CounterfactualConditionPayloadV1>(json!({
            "type": "relaxRequiredRule",
            "ruleId": uuid(1)
        }))
        .is_err()
    );
    let unchanged_hash = hash("unchanged-model");
    assert!(matches!(
        CounterfactualCompilationBindingV1::new(
            unchanged_hash.clone(),
            hash("condition"),
            unchanged_hash,
            hash("objective"),
        ),
        Err(DomainContractError::InvalidExplanationContract(
            "unchanged counterfactual model"
        ))
    ));
    Ok(())
}

#[test]
fn counterfactual_request_hash_excludes_job_request_ids_and_timestamps()
-> Result<(), Box<dyn Error>> {
    let fixture = counterfactual_fixture()?;
    let second = CounterfactualJobRequestV1::new(
        uuid(0x30).parse()?,
        uuid(0x31).parse()?,
        fixture.request.semantics.clone(),
        fixture.request.condition.clone(),
        "2027-01-01T00:00:00Z".parse()?,
    )?;
    assert_eq!(fixture.request.request_hash, second.request_hash);
    assert_ne!(fixture.request.job_id, second.job_id);
    assert_ne!(fixture.request.request_id, second.request_id);
    assert_ne!(fixture.request.created_at, second.created_at);

    let mismatched =
        CounterfactualConditionV1::new(CounterfactualConditionPayloadV1::ForbidAssignmentValue {
            assignment_id: DomainAssignmentId::new("assignment.target")?,
            value: AssignmentValue::Boolean(true),
        })?;
    assert!(matches!(
        CounterfactualJobRequestV1::new(
            uuid(0x32).parse()?,
            uuid(0x33).parse()?,
            fixture.request.semantics.clone(),
            mismatched,
            "2027-01-01T00:00:00Z".parse()?,
        ),
        Err(DomainContractError::InvalidExplanationContract(
            "counterfactual request condition binding"
        ))
    ));
    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
fn counterfactual_authority_substitution_is_rejected() -> Result<(), Box<dyn Error>> {
    let fixture = counterfactual_fixture()?;

    let replacement_request = CounterfactualJobRequestV1::new(
        uuid(0x34).parse()?,
        uuid(0x35).parse()?,
        fixture.request.semantics.clone(),
        fixture.request.condition.clone(),
        "2026-09-03T12:00:00Z".parse()?,
    )?;
    assert!(matches!(
        CounterfactualJobRecordV1::new(
            replacement_request,
            CounterfactualJobState::Completed,
            Some("2026-09-03T12:00:01Z".parse()?),
            Some("2026-09-03T12:00:03Z".parse()?),
            None,
            None,
            Some(fixture.result.clone()),
            None,
        ),
        Err(DomainContractError::InvalidExplanationContract(
            "counterfactual job result binding"
        ))
    ));

    let mut detached_base = fixture.result.clone();
    detached_base.base_run_input = detached_base.run_input.clone();
    assert!(matches!(
        detached_base.validate(),
        Err(DomainContractError::InvalidExplanationContract(
            "counterfactual base run binding"
        ))
    ));

    let reused_run_input = rebuild_run_input(
        &fixture.result.run_input,
        fixture.result.base_run_input.run_id,
        fixture.result.run_input.compiler_version.clone(),
    )?;
    let reused_run_manifest = infeasible_manifest(&reused_run_input)?;
    assert!(matches!(
        CounterfactualResultV1::new(
            fixture.request.clone(),
            fixture.result.base_run_input.clone(),
            fixture.result.base_run_manifest.clone(),
            fixture.result.compilation.clone(),
            reused_run_input,
            reused_run_manifest,
            CounterfactualConclusionV1::ProvenImpossible,
        ),
        Err(DomainContractError::InvalidExplanationContract(
            "counterfactual derived run binding"
        ))
    ));

    let changed_compiler_input = rebuild_run_input(
        &fixture.result.run_input,
        fixture.result.run_input.run_id,
        "2.0.0".to_owned(),
    )?;
    let changed_compiler_manifest = infeasible_manifest(&changed_compiler_input)?;
    assert!(matches!(
        CounterfactualResultV1::new(
            fixture.request.clone(),
            fixture.result.base_run_input.clone(),
            fixture.result.base_run_manifest.clone(),
            fixture.result.compilation.clone(),
            changed_compiler_input,
            changed_compiler_manifest,
            CounterfactualConclusionV1::ProvenImpossible,
        ),
        Err(DomainContractError::InvalidExplanationContract(
            "counterfactual derived run binding"
        ))
    ));

    let alternative = result_ref(2)?;
    let accepted_manifest = RunManifestV1::new(
        fixture.result.run_input.run_id,
        fixture.result.run_input.checksum.clone(),
        RunTerminalOutcomeV1::Accepted {
            status: SolveStatus::Feasible,
            solution_id: alternative.solution_id,
            accepted_result_checksum: alternative.result_checksum.clone(),
            verification_checksum: hash("alternative-verification"),
        },
        "2026-09-03T12:00:01Z".parse()?,
        "2026-09-03T12:00:02Z".parse()?,
        Some(DurationMillis::new(1_000)?),
        Some(DurationMillis::new(500)?),
        Some(DurationMillis::new(800)?),
        RunPhaseTimingsV1::default(),
        Vec::new(),
    )?;
    let comparison = counterfactual_comparison(
        &fixture.result,
        fixture.request.semantics.base.clone(),
        alternative.clone(),
    )?;
    let verified = CounterfactualResultV1::new(
        fixture.request.clone(),
        fixture.result.base_run_input.clone(),
        fixture.result.base_run_manifest.clone(),
        fixture.result.compilation.clone(),
        fixture.result.run_input.clone(),
        accepted_manifest.clone(),
        CounterfactualConclusionV1::VerifiedAlternative {
            alternative: alternative.clone(),
            comparison: Box::new(comparison),
            ordering: ComparisonOrdering::Equivalent,
        },
    )?;
    verified.validate()?;

    let substituted_result = result_ref(3)?;
    let substituted_result_comparison = counterfactual_comparison(
        &fixture.result,
        fixture.request.semantics.base.clone(),
        substituted_result.clone(),
    )?;
    assert!(matches!(
        CounterfactualResultV1::new(
            fixture.request.clone(),
            fixture.result.base_run_input.clone(),
            fixture.result.base_run_manifest.clone(),
            fixture.result.compilation.clone(),
            fixture.result.run_input.clone(),
            accepted_manifest.clone(),
            CounterfactualConclusionV1::VerifiedAlternative {
                alternative: substituted_result,
                comparison: Box::new(substituted_result_comparison),
                ordering: ComparisonOrdering::Equivalent,
            },
        ),
        Err(DomainContractError::InvalidExplanationContract(
            "counterfactual alternative binding"
        ))
    ));

    let substituted_comparison =
        counterfactual_comparison(&fixture.result, result_ref(4)?, alternative.clone())?;
    assert!(matches!(
        CounterfactualResultV1::new(
            fixture.request.clone(),
            fixture.result.base_run_input.clone(),
            fixture.result.base_run_manifest.clone(),
            fixture.result.compilation.clone(),
            fixture.result.run_input.clone(),
            accepted_manifest,
            CounterfactualConclusionV1::VerifiedAlternative {
                alternative,
                comparison: Box::new(substituted_comparison),
                ordering: ComparisonOrdering::Equivalent,
            },
        ),
        Err(DomainContractError::InvalidExplanationContract(
            "counterfactual alternative binding"
        ))
    ));
    Ok(())
}

#[test]
fn every_valid_counterfactual_job_state_timestamp_matrix_is_accepted() -> Result<(), Box<dyn Error>>
{
    let fixture = counterfactual_fixture()?;
    let started = "2026-09-03T12:00:01Z".parse()?;
    let finished = "2026-09-03T12:00:03Z".parse()?;
    let valid = vec![
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Queued,
            None,
            None,
            None,
            None,
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Running,
            Some(started),
            None,
            None,
            None,
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Running,
            Some(started),
            None,
            Some(uuid(0x32).parse()?),
            Some(started),
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Completed,
            Some(started),
            Some(finished),
            None,
            None,
            Some(fixture.result.clone()),
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Failed,
            None,
            Some(finished),
            None,
            None,
            None,
            Some(CounterfactualJobErrorV1 {
                kind: CounterfactualFailureKind::CompilationFailed,
            }),
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Cancelled,
            None,
            Some(finished),
            Some(uuid(0x33).parse()?),
            Some(started),
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request,
            CounterfactualJobState::Interrupted,
            Some(started),
            Some(finished),
            None,
            None,
            None,
            None,
        ),
    ];
    for record in valid {
        let record = record?;
        assert_eq!(
            CounterfactualJobRecordV1::from_json(&serde_json::to_vec(&record)?)?,
            record
        );
    }
    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
fn every_invalid_counterfactual_job_state_or_timestamp_shape_is_rejected()
-> Result<(), Box<dyn Error>> {
    let fixture = counterfactual_fixture()?;
    let started = "2026-09-03T12:00:01Z".parse()?;
    let finished = "2026-09-03T12:00:03Z".parse()?;
    let error = CounterfactualJobErrorV1 {
        kind: CounterfactualFailureKind::BackendFailed,
    };
    let mut over_budget_semantics = fixture.request.semantics.clone();
    over_budget_semantics.total_budget_milliseconds = DurationMillis::new(4_999)?;
    let over_budget_request = CounterfactualJobRequestV1::new(
        fixture.request.job_id,
        fixture.request.request_id,
        over_budget_semantics,
        fixture.request.condition.clone(),
        fixture.request.created_at,
    )?;
    let invalid = vec![
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Queued,
            Some(started),
            None,
            None,
            None,
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Queued,
            None,
            None,
            Some(uuid(0x41).parse()?),
            Some(started),
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            over_budget_request,
            CounterfactualJobState::Completed,
            Some(started),
            Some(finished),
            None,
            None,
            Some(fixture.result.clone()),
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Running,
            Some(started),
            Some(finished),
            None,
            None,
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Completed,
            Some(started),
            Some(finished),
            None,
            None,
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Completed,
            Some(started),
            Some(finished),
            Some(uuid(0x42).parse()?),
            Some(started),
            Some(fixture.result.clone()),
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Failed,
            None,
            Some(finished),
            None,
            None,
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Failed,
            None,
            Some(finished),
            Some(uuid(0x43).parse()?),
            Some(started),
            None,
            Some(error),
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Cancelled,
            None,
            Some(finished),
            None,
            None,
            None,
            Some(error),
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Interrupted,
            None,
            Some(finished),
            None,
            None,
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Interrupted,
            Some(started),
            Some(finished),
            Some(uuid(0x44).parse()?),
            Some(started),
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Cancelled,
            None,
            Some(finished),
            Some(uuid(0x40).parse()?),
            None,
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request.clone(),
            CounterfactualJobState::Cancelled,
            None,
            Some(finished),
            None,
            None,
            None,
            None,
        ),
        CounterfactualJobRecordV1::new(
            fixture.request,
            CounterfactualJobState::Running,
            Some("2026-09-02T12:00:00Z".parse()?),
            None,
            None,
            None,
            None,
            None,
        ),
    ];
    assert!(invalid.into_iter().all(|record| record.is_err()));
    Ok(())
}

#[test]
fn conflict_minimality_and_group_identity_are_derived_and_validated() -> Result<(), Box<dyn Error>>
{
    let group = ConflictGroupV1 {
        group_id: AssumptionGroupId::new("assumption.required")?,
        required_rules: vec![uuid(1).parse()?],
    };
    let evidence = ExplanationEvidenceV1::new(ExplanationEvidencePayloadV1::Infeasibility {
        infeasibility: InfeasibilityEvidenceV1::Conflict {
            groups: vec![group],
            minimality: ConflictMinimality::ProvenMinimal,
            shrink: ConflictShrinkSummaryV1 {
                initial_group_count: 1,
                remaining_group_count: 1,
                attempted_trials: 1,
                max_trials: 4,
                stop_reason: ConflictShrinkStopReason::Completed,
            },
        },
    })?;
    assert_eq!(
        evidence.certainty,
        ExplanationCertainty::ProvenMinimalConflict
    );

    let invalid_core = ExplanationEvidenceV1::new(ExplanationEvidencePayloadV1::Infeasibility {
        infeasibility: InfeasibilityEvidenceV1::Unavailable {
            reason: ConflictUnavailableReason::InvalidAssumptionCore,
        },
    })?;
    assert_eq!(invalid_core.certainty, ExplanationCertainty::Unavailable);
    assert_eq!(
        ExplanationEvidenceV1::from_json(&serde_json::to_vec(&invalid_core)?)?,
        invalid_core
    );
    assert_eq!(
        serde_json::to_value(ConflictUnavailableReason::InvalidAssumptionCore)?,
        json!("invalidAssumptionCore")
    );

    let invalid = InfeasibilityEvidenceV1::Conflict {
        groups: Vec::new(),
        minimality: ConflictMinimality::Sufficient,
        shrink: ConflictShrinkSummaryV1 {
            initial_group_count: 1,
            remaining_group_count: 1,
            attempted_trials: 0,
            max_trials: 0,
            stop_reason: ConflictShrinkStopReason::NotAttempted,
        },
    };
    assert!(
        ExplanationEvidenceV1::new(ExplanationEvidencePayloadV1::Infeasibility {
            infeasibility: invalid,
        })
        .is_err()
    );
    Ok(())
}

#[test]
fn cross_revision_comparison_retains_both_bindings_and_rejects_tampering()
-> Result<(), Box<dyn Error>> {
    let comparison = comparison()?;
    assert_eq!(comparison.base.scenario_revision, 7);
    assert_eq!(comparison.candidate.scenario_revision, 8);
    assert_ne!(
        comparison.base.document_hash,
        comparison.candidate.document_hash
    );
    comparison.validate()?;

    let mut tampered: Value = serde_json::to_value(&comparison)?;
    tampered["ordering"] = json!("better");

    let mut same_revision_wrong_scope = comparison.clone();
    same_revision_wrong_scope.candidate.scenario_revision =
        same_revision_wrong_scope.base.scenario_revision;
    assert!(matches!(
        same_revision_wrong_scope.validate(),
        Err(DomainContractError::InvalidExplanationContract(
            "same-revision verification scope"
        ))
    ));
    assert!(matches!(
        SolutionComparisonV1::from_json(&serde_json::to_vec(&tampered)?),
        Err(DomainContractError::InvalidExplanationContract(
            "comparison ordering"
        ))
    ));
    Ok(())
}
