use eutheto_domain_ir::*;
use eutheto_types::{
    BackendId, BackendSelection, DurationMillis, ExplanationMode, IanaTimeZone, PackId,
    PreservationPolicy, REVISION_MAX_V1, ReproducibilityMode, RequestId, ResourceLimits,
    Rfc3339Timestamp, RuleId, ScenarioId, ScenarioSnapshotId, SolutionId, SolveMode, SolveOptions,
    SolveRunId, SolveStatus, WorkerThreadPolicy, extract_result_dependency, extract_result_id,
};
use proptest::prelude::*;
use serde_json::json;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::error::Error;

fn score(value: i64) -> Result<ScoreVector, Box<dyn Error>> {
    Ok(ScoreVector {
        feasibility: 0,
        levels: vec![ScoreLevelValue {
            level_id: ScoreLevelId::new("score.preference")?,
            value,
            direction: OptimizationDirection::Minimize,
            category_breakdown: BTreeMap::new(),
        }],
    })
}

fn solution(revision: u64) -> Result<NormalizedSolution, Box<dyn Error>> {
    Ok(NormalizedSolution {
        schema_version: NORMALIZED_SOLUTION_SCHEMA_VERSION,
        pack_id: PackId::new("official.synthetic")?,
        scenario_id: "01890a5d-ac96-7b64-9f74-bbfcf30f9f80".parse::<ScenarioId>()?,
        scenario_revision: revision,
        projection_version: 1,
        solution_id: "01890a5d-ac96-7b64-9f74-bbfcf30f9f81".parse::<SolutionId>()?,
        assignments: Vec::new(),
    })
}

fn rule(suffix: u8) -> Result<RuleId, Box<dyn Error>> {
    Ok(format!("01890a5d-ac96-7b64-9f74-bbfcf30f9f{suffix:02x}").parse::<RuleId>()?)
}

fn entity(suffix: &str) -> Result<DomainEntityRef, Box<dyn Error>> {
    Ok(DomainEntityRef {
        kind: DomainEntityKindId::new("school.person")?,
        id: DomainEntityId::new(format!("school.{suffix}"))?,
    })
}

fn binding(rule_id: RuleId, meaning: &[u8]) -> RequiredRuleBinding {
    RequiredRuleBinding {
        rule_id,
        semantic_hash: blake3_hex(meaning),
    }
}

fn evaluation(rule_id: RuleId, satisfied: bool) -> Result<RuleEvaluation, Box<dyn Error>> {
    Ok(RuleEvaluation {
        rule_id,
        satisfied,
        affected_entities: vec![entity("person-b")?, entity("person-a")?],
        message_key: "official.synthetic.required".to_owned(),
        expected: BTreeMap::from([(
            VerificationFactId::new("fact.required")?,
            VerificationValue::Boolean(true),
        )]),
        observed: BTreeMap::from([(
            VerificationFactId::new("fact.required")?,
            VerificationValue::Boolean(satisfied),
        )]),
        evidence: vec![
            DomainEvidenceId::new("evidence.second")?,
            DomainEvidenceId::new("evidence.first")?,
        ],
    })
}

fn warning(suffix: &str) -> Result<VerificationWarning, Box<dyn Error>> {
    Ok(VerificationWarning {
        id: VerificationWarningId::new(format!("warning.{suffix}"))?,
        message_key: "official.synthetic.warning".to_owned(),
        affected_entities: vec![entity("person-b")?, entity("person-a")?],
        facts: BTreeMap::from([(
            VerificationFactId::new("fact.detail")?,
            VerificationValue::Text("bounded detail".to_owned()),
        )]),
    })
}

fn scope(revision: u64) -> Result<VerificationScope, Box<dyn Error>> {
    let scenario_id = solution(revision)?.scenario_id;
    Ok(VerificationScope::new(
        scenario_id,
        revision,
        vec![binding(rule(2)?, b"second"), binding(rule(1)?, b"first")],
    )?)
}

fn context(
    solution: &NormalizedSolution,
    scope: &VerificationScope,
) -> Result<VerificationContextV1, Box<dyn Error>> {
    Ok(VerificationContextV1::new(
        solution.scenario_id,
        solution.scenario_revision,
        blake3_hex(b"document"),
        blake3_hex(b"planning-model"),
        solution.canonical_hash()?,
        scope.checksum.clone(),
    )?)
}

fn report(
    solution: &NormalizedSolution,
    feasibility: i64,
    satisfied: bool,
) -> Result<VerificationReport, Box<dyn Error>> {
    let scope = scope(solution.scenario_revision)?;
    let context = context(solution, &scope)?;
    let mut authoritative_score = score(4)?;
    authoritative_score.feasibility = feasibility;
    Ok(VerificationReport::new(
        &context,
        vec![
            evaluation(rule(2)?, satisfied)?,
            evaluation(rule(1)?, true)?,
        ],
        authoritative_score,
        vec![warning("second")?, warning("first")?],
        BTreeMap::from([
            (
                MetricId::new("metric.ratio")?,
                MetricValue::Ratio {
                    numerator: 1,
                    denominator: 2,
                },
            ),
            (MetricId::new("metric.total")?, MetricValue::Integer(4)),
        ]),
    )?)
}

fn solve_options() -> Result<SolveOptions, Box<dyn Error>> {
    Ok(SolveOptions {
        backend: BackendSelection::Auto,
        mode: SolveMode::Balanced,
        time_limit_milliseconds: DurationMillis::new(30_000)?,
        memory_limit_bytes: Some(64 * 1024 * 1024),
        worker_threads: WorkerThreadPolicy::Exact(1),
        random_seed: 7,
        solution_limit: Some(1),
        stop_after_first_feasible: true,
        collect_intermediate_solutions: false,
        explanation_mode: ExplanationMode::Standard,
        preserve_existing: PreservationPolicy::None,
        reproducibility: ReproducibilityMode::Deterministic,
        resource_limits: ResourceLimits {
            max_entities: 1_000,
            max_rules: 1_000,
            max_variables: 10_000,
            max_constraints: 10_000,
        },
    })
}

fn run_id(suffix: u8) -> Result<SolveRunId, Box<dyn Error>> {
    Ok(format!("01890a5d-ac96-7b64-9f74-bbfcf30f9f{suffix:02x}").parse()?)
}

fn run_input_at(
    accepted: &AcceptedResult,
    run_id: SolveRunId,
    snapshot_document_hash: String,
    model_hash: String,
    snapshot_created_at: Rfc3339Timestamp,
) -> Result<RunInputV1, Box<dyn Error>> {
    Ok(RunInputV1::new(
        run_id,
        "01890a5d-ac96-7b64-9f74-bbfcf30f9f83".parse::<RequestId>()?,
        accepted.solution.scenario_id,
        accepted.solution.scenario_revision,
        "01890a5d-ac96-7b64-9f74-bbfcf30f9f84".parse::<ScenarioSnapshotId>()?,
        snapshot_document_hash,
        snapshot_created_at,
        accepted.solution.pack_id.clone(),
        1,
        1,
        "1.2.3".to_owned(),
        "0.1.0".to_owned(),
        BackendId::new("ortools.cp-sat")?,
        "9.15.6755".to_owned(),
        "0.1.0".to_owned(),
        "0.1.0".to_owned(),
        "9.15.6755".to_owned(),
        1,
        0,
        model_hash,
        blake3_hex(b"objective-policy"),
        solve_options()?,
        "America/Chicago".parse::<IanaTimeZone>()?,
        Some(blake3_hex(b"temporary-conditions")),
    )?)
}

fn run_input_for(
    accepted: &AcceptedResult,
    run_id: SolveRunId,
    snapshot_document_hash: String,
    model_hash: String,
) -> Result<RunInputV1, Box<dyn Error>> {
    run_input_at(
        accepted,
        run_id,
        snapshot_document_hash,
        model_hash,
        "2026-09-03T12:00:00Z".parse()?,
    )
}

fn accepted_contract() -> Result<AcceptedResult, Box<dyn Error>> {
    let solution = solution(7)?;
    Ok(AcceptedResult::new(
        solution.clone(),
        report(&solution, 0, true)?,
    )?)
}

fn manifest_for(
    run_id: SolveRunId,
    run_input_checksum: String,
    outcome: RunTerminalOutcomeV1,
    first_verified_feasible_milliseconds: Option<DurationMillis>,
    verification_warnings: Vec<VerificationWarning>,
) -> Result<RunManifestV1, Box<dyn Error>> {
    Ok(RunManifestV1::new(
        run_id,
        run_input_checksum,
        outcome,
        "2026-09-03T12:00:01Z".parse::<Rfc3339Timestamp>()?,
        "2026-09-03T12:00:02Z".parse::<Rfc3339Timestamp>()?,
        Some(DurationMillis::new(1_000)?),
        Some(DurationMillis::new(100)?),
        first_verified_feasible_milliseconds,
        RunPhaseTimingsV1 {
            compile_milliseconds: Some(DurationMillis::new(10)?),
            backend_milliseconds: Some(DurationMillis::new(400)?),
            projection_milliseconds: Some(DurationMillis::new(20)?),
            structural_validation_milliseconds: Some(DurationMillis::new(15)?),
            score_recomputation_milliseconds: Some(DurationMillis::new(15)?),
            required_rule_verification_milliseconds: Some(DurationMillis::new(30)?),
            evidence_persistence_milliseconds: Some(DurationMillis::new(10)?),
            optional_explanation_milliseconds: None,
        },
        verification_warnings,
    )?)
}

fn accepted_outcome(accepted: &AcceptedResult) -> RunTerminalOutcomeV1 {
    RunTerminalOutcomeV1::Accepted {
        status: SolveStatus::Feasible,
        solution_id: accepted.solution.solution_id,
        accepted_result_checksum: accepted.checksum.clone(),
        verification_checksum: accepted.verification.checksum.clone(),
    }
}

fn accepted_manifest(
    input: &RunInputV1,
    accepted: &AcceptedResult,
) -> Result<RunManifestV1, Box<dyn Error>> {
    manifest_for(
        input.run_id,
        input.checksum.clone(),
        accepted_outcome(accepted),
        Some(DurationMillis::new(500)?),
        accepted.verification.warnings.clone(),
    )
}

fn portable_result() -> Result<PortableAcceptedResultV2, Box<dyn Error>> {
    let accepted = accepted_contract()?;
    let input = run_input_for(
        &accepted,
        run_id(0x82)?,
        accepted.verification.document_hash.clone(),
        accepted.verification.planning_model_hash.clone(),
    )?;
    let manifest = accepted_manifest(&input, &accepted)?;
    Ok(PortableAcceptedResultV2::new(
        input,
        manifest,
        accepted,
        BTreeMap::from([(
            DomainEvidenceId::new("evidence.first")?,
            VerificationValue::Integer(1),
        )]),
    )?)
}

#[test]
fn ids_accept_required_grammar_and_boundaries() -> Result<(), Box<dyn Error>> {
    let id = DomainAssignmentId::new("school_2.section-a")?;
    assert_eq!(id.as_str(), "school_2.section-a");
    assert!(DomainAssignmentId::new("School.section").is_err());
    assert!(DomainAssignmentId::new("school").is_err());
    assert!(DomainAssignmentId::new(format!("aa.{}", "b".repeat(158))).is_err());
    assert!(VerificationFactId::new("fact.required").is_ok());
    assert!(MetricId::new("metric.total").is_ok());
    Ok(())
}

#[test]
fn interval_and_absent_are_distinct_and_checked() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        AssignedInterval::new(5, 0, 5)?,
        AssignedInterval {
            start: 5,
            duration: 0,
            end: 5
        }
    );
    assert!(AssignedInterval::new(5, -1, 4).is_err());
    assert!(AssignedInterval::new(i64::MAX, 1, i64::MIN).is_err());
    assert_ne!(AssignmentValue::Absent, AssignmentValue::Boolean(false));
    Ok(())
}

#[test]
fn normalized_solution_v1_is_unchanged_and_hashes_canonically() -> Result<(), Box<dyn Error>> {
    let mut value = solution(3)?;
    let assignment = DomainAssignment {
        id: DomainAssignmentId::new("school.second")?,
        entity: entity("section-a")?,
        value: AssignmentValue::Integer(1),
        evidence: vec![
            DomainEvidenceId::new("evidence.z")?,
            DomainEvidenceId::new("evidence.a")?,
        ],
    };
    value.assignments = vec![assignment];
    assert_eq!(
        value.validate(),
        Err(DomainContractError::NonCanonicalEvidence)
    );
    value.canonicalize()?;
    let first_hash = value.canonical_hash()?;
    assert_eq!(first_hash.len(), 64);
    assert!(
        first_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    );
    let json = serde_json::to_value(&value)?;
    assert!(json.get("checksum").is_none());
    assert_eq!(value.schema_version, 1);
    let duplicate = value.assignments[0].clone();
    value.assignments.push(duplicate);
    assert!(matches!(
        value.canonicalize(),
        Err(DomainContractError::DuplicateAssignment(_))
    ));
    Ok(())
}

#[test]
fn scope_sorts_bindings_rejects_duplicates_and_binds_revision() -> Result<(), Box<dyn Error>> {
    let first = scope(7)?;
    assert_eq!(first.required_rules[0].rule_id, rule(1)?);
    assert_eq!(first.required_rules[1].rule_id, rule(2)?);
    first.validate()?;

    let reordered = VerificationScope::new(
        first.scenario_id,
        7,
        vec![binding(rule(1)?, b"first"), binding(rule(2)?, b"second")],
    )?;
    assert_eq!(first.checksum, reordered.checksum);
    assert_ne!(first.checksum, scope(8)?.checksum);
    assert_eq!(
        VerificationScope::new(
            first.scenario_id,
            7,
            vec![binding(rule(1)?, b"first"), binding(rule(1)?, b"duplicate")]
        ),
        Err(DomainContractError::DuplicateRequiredRule(rule(1)?))
    );
    assert_eq!(
        VerificationScope::new(
            first.scenario_id,
            7,
            vec![RequiredRuleBinding {
                rule_id: rule(1)?,
                semantic_hash: "A".repeat(64)
            }]
        ),
        Err(DomainContractError::InvalidBlake3)
    );
    Ok(())
}

#[test]
fn report_constructor_canonicalizes_and_hashes_typed_content() -> Result<(), Box<dyn Error>> {
    let solution = solution(7)?;
    let report = report(&solution, 0, true)?;
    assert!(report.accepted);
    assert_eq!(report.schema_version, 2);
    assert_eq!(report.required_rule_results[0].rule_id, rule(1)?);
    assert_eq!(
        report.warnings[0].id,
        VerificationWarningId::new("warning.first")?
    );
    assert_eq!(
        report.required_rule_results[0].affected_entities[0],
        entity("person-a")?
    );
    assert_eq!(
        report.required_rule_results[0].evidence[0],
        DomainEvidenceId::new("evidence.first")?
    );
    report.validate()?;

    let encoded = serde_json::to_value(&report)?;
    assert!(encoded["metrics"]["metric.total"].get("type").is_some());
    assert!(serde_json::from_value::<VerificationValue>(json!({"untyped": true})).is_err());
    Ok(())
}

#[test]
fn failed_rule_remains_visible_and_controls_report_acceptance() -> Result<(), Box<dyn Error>> {
    let solution = solution(7)?;
    let report = report(&solution, 0, false)?;
    assert!(!report.accepted);
    assert_eq!(report.required_rule_results.len(), 2);
    assert!(
        report
            .required_rule_results
            .iter()
            .any(|result| !result.satisfied)
    );
    report.validate()?;
    assert_eq!(
        AcceptedResult::new(solution, report),
        Err(DomainContractError::NotVerifiedFeasible)
    );
    Ok(())
}

#[test]
fn zero_feasibility_is_enforced_only_at_acceptance_boundary() -> Result<(), Box<dyn Error>> {
    let solution = solution(7)?;
    let inconsistent = report(&solution, 1, true)?;
    assert!(inconsistent.accepted);
    inconsistent.validate()?;
    assert_eq!(
        AcceptedResult::new(solution.clone(), inconsistent),
        Err(DomainContractError::NotVerifiedFeasible)
    );

    let accepted_report = report(&solution, 0, true)?;
    let accepted = AcceptedResult::new(solution, accepted_report)?;
    accepted.validate()?;
    assert_eq!(accepted.schema_version, 2);
    assert_eq!(accepted.checksum.len(), 64);
    Ok(())
}

#[test]
fn accepted_result_rejects_every_solution_report_binding_mismatch() -> Result<(), Box<dyn Error>> {
    let original = solution(7)?;
    let report = report(&original, 0, true)?;
    assert!(AcceptedResult::new(original.clone(), report.clone()).is_ok());

    let mut wrong_revision = original.clone();
    wrong_revision.scenario_revision = 8;
    assert_eq!(
        AcceptedResult::new(wrong_revision, report.clone()),
        Err(DomainContractError::VerificationBindingMismatch)
    );

    let mut wrong_solution = original;
    wrong_solution.solution_id = "01890a5d-ac96-7b64-9f74-bbfcf30f9f99".parse()?;
    assert_eq!(
        AcceptedResult::new(wrong_solution, report),
        Err(DomainContractError::VerificationBindingMismatch)
    );
    Ok(())
}

#[test]
fn current_result_contracts_reject_nonportable_revisions() -> Result<(), Box<dyn Error>> {
    let invalid_revision = REVISION_MAX_V1 + 1;
    let invalid_solution = solution(invalid_revision)?;
    assert_eq!(
        invalid_solution.validate(),
        Err(DomainContractError::LimitExceeded("scenario revision"))
    );

    let scenario_id = solution(7)?.scenario_id;
    assert_eq!(
        VerificationScope::new(scenario_id, invalid_revision, Vec::new()),
        Err(DomainContractError::LimitExceeded("scenario revision"))
    );

    let valid_scope = scope(7)?;
    assert_eq!(
        VerificationContextV1::new(
            scenario_id,
            invalid_revision,
            blake3_hex(b"document"),
            blake3_hex(b"planning-model"),
            blake3_hex(b"solution"),
            valid_scope.checksum,
        ),
        Err(DomainContractError::LimitExceeded("scenario revision"))
    );

    let valid_solution = solution(7)?;
    let mut invalid_report = report(&valid_solution, 0, true)?;
    invalid_report.evaluated_revision = invalid_revision;
    assert_eq!(
        invalid_report.validate(),
        Err(DomainContractError::LimitExceeded("scenario revision"))
    );
    Ok(())
}

#[test]
fn checksum_mutation_is_rejected_at_every_checksummed_boundary() -> Result<(), Box<dyn Error>> {
    let mut scope = scope(7)?;
    scope.required_rules[0].semantic_hash = blake3_hex(b"mutated");
    assert_eq!(scope.validate(), Err(DomainContractError::ChecksumMismatch));

    let solution = solution(7)?;
    let mut mutated_report = report(&solution, 0, true)?;
    mutated_report.planning_model_hash = blake3_hex(b"different-model");
    assert_eq!(
        mutated_report.validate(),
        Err(DomainContractError::ChecksumMismatch)
    );

    let mut accepted = AcceptedResult::new(solution.clone(), report(&solution, 0, true)?)?;
    accepted.checksum = blake3_hex(b"forged-result");
    assert_eq!(
        accepted.validate(),
        Err(DomainContractError::ChecksumMismatch)
    );
    Ok(())
}

#[test]
fn deserialized_noncanonical_collections_are_rejected_before_checksum() -> Result<(), Box<dyn Error>>
{
    let solution = solution(7)?;
    let mut noncanonical_rules = report(&solution, 0, true)?;
    noncanonical_rules.required_rule_results.reverse();
    assert_eq!(
        noncanonical_rules.validate(),
        Err(DomainContractError::NonCanonicalRuleEvaluations)
    );

    let mut noncanonical_warnings = report(&solution, 0, true)?;
    noncanonical_warnings.warnings.reverse();
    assert_eq!(
        noncanonical_warnings.validate(),
        Err(DomainContractError::NonCanonicalVerificationWarnings)
    );

    let scope = scope(7)?;
    let context = context(&solution, &scope)?;
    let duplicate = evaluation(rule(1)?, true)?;
    assert_eq!(
        VerificationReport::new(
            &context,
            vec![duplicate.clone(), duplicate],
            score(0)?,
            Vec::new(),
            BTreeMap::new()
        ),
        Err(DomainContractError::DuplicateRuleEvaluation(rule(1)?))
    );
    Ok(())
}

#[test]
fn typed_values_and_metrics_enforce_bounds() -> Result<(), Box<dyn Error>> {
    let solution = solution(7)?;
    let scope = scope(7)?;
    let context = context(&solution, &scope)?;
    let mut invalid_text = evaluation(rule(1)?, true)?;
    invalid_text.expected.insert(
        VerificationFactId::new("fact.text")?,
        VerificationValue::Text(String::new()),
    );
    assert_eq!(
        VerificationReport::new(
            &context,
            vec![invalid_text],
            score(0)?,
            Vec::new(),
            BTreeMap::new()
        ),
        Err(DomainContractError::InvalidVerificationText)
    );
    assert_eq!(
        VerificationReport::new(
            &context,
            Vec::new(),
            score(0)?,
            Vec::new(),
            BTreeMap::from([(
                MetricId::new("metric.ratio")?,
                MetricValue::Ratio {
                    numerator: 1,
                    denominator: 0
                }
            )])
        ),
        Err(DomainContractError::InvalidMetricRatio)
    );
    Ok(())
}

#[test]
fn strict_json_rejects_unknown_fields_and_unknown_versions() -> Result<(), Box<dyn Error>> {
    let solution = solution(1)?;
    let mut json = serde_json::to_value(&solution)?;
    json.as_object_mut()
        .ok_or("solution JSON is not an object")?
        .insert("futureField".to_owned(), serde_json::Value::Bool(true));
    assert!(matches!(
        NormalizedSolution::from_json(&serde_json::to_vec(&json)?),
        Err(DomainContractError::MalformedJson(_))
    ));

    let mut report = report(&solution, 0, true)?;
    report.schema_version = 3;
    assert_eq!(
        report.validate(),
        Err(DomainContractError::UnsupportedVersion(3))
    );

    let mut context = context(&solution, &scope(1)?)?;
    context.document_hash = "ABC".to_owned();
    assert_eq!(context.validate(), Err(DomainContractError::InvalidBlake3));
    Ok(())
}

#[test]
fn legacy_v1_decodes_but_cannot_validate_as_current() -> Result<(), Box<dyn Error>> {
    let legacy_report = LegacyVerificationReportV1 {
        schema_version: 1,
        scenario_revision: 7,
        feasible: true,
        issues: vec![LegacyVerificationIssueV1 {
            id: VerificationIssueId::new("legacy.issue")?,
            severity: LegacyVerificationSeverityV1::Warning,
            message_key: "legacy.warning".to_owned(),
            entities: Vec::new(),
            evidence: Vec::new(),
        }],
        score: Some(score(0)?),
    };
    let legacy = LegacyAcceptedResultV1 {
        schema_version: 1,
        solution: solution(7)?,
        verification: legacy_report,
    };
    let bytes = serde_json::to_vec(&legacy)?;
    let decoded = LegacyAcceptedResultV1::from_json(&bytes)?;
    assert_eq!(decoded, legacy);
    assert!(matches!(
        AcceptedResult::from_json(&bytes),
        Err(DomainContractError::MalformedJson(_))
    ));
    let legacy_report_bytes = serde_json::to_vec(&legacy.verification)?;
    assert!(matches!(
        VerificationReport::from_json(&legacy_report_bytes),
        Err(DomainContractError::MalformedJson(_))
    ));
    let mut wide_legacy = legacy.clone();
    let wide_score = wide_legacy
        .verification
        .score
        .as_mut()
        .ok_or_else(|| std::io::Error::other("legacy fixture score missing"))?;
    wide_score.levels.clear();
    for index in 0..=MAX_SCORE_LEVELS {
        wide_score.levels.push(ScoreLevelValue {
            level_id: ScoreLevelId::new(format!("legacy.score.level.{index}"))?,
            value: i64::try_from(index)?,
            direction: OptimizationDirection::Minimize,
            category_breakdown: BTreeMap::new(),
        });
    }
    assert!(wide_score.validate_shape().is_ok());
    assert_eq!(
        wide_score.validate_current_shape(),
        Err(DomainContractError::LimitExceeded("score levels"))
    );
    assert_eq!(
        LegacyAcceptedResultV1::from_json(&serde_json::to_vec(&wide_legacy)?)?,
        wide_legacy
    );
    let mut category_legacy = legacy.clone();
    let category_score = category_legacy
        .verification
        .score
        .as_mut()
        .ok_or_else(|| std::io::Error::other("legacy fixture score missing"))?;
    for index in 0..=MAX_SCORE_CATEGORIES_PER_LEVEL {
        category_score.levels[0].category_breakdown.insert(
            ScoreCategoryId::new(format!("legacy.score.category.{index}"))?,
            i64::try_from(index)?,
        );
    }
    assert!(category_score.validate_shape().is_ok());
    assert_eq!(
        category_score.validate_current_shape(),
        Err(DomainContractError::LimitExceeded("score categories"))
    );
    assert_eq!(
        LegacyAcceptedResultV1::from_json(&serde_json::to_vec(&category_legacy)?)?,
        category_legacy
    );
    let mut newer = legacy;
    newer.schema_version = 9;
    assert_eq!(
        LegacyAcceptedResultV1::from_json(&serde_json::to_vec(&newer)?),
        Err(DomainContractError::UnsupportedVersion(9))
    );
    Ok(())
}

#[test]
fn run_and_portable_contracts_round_trip_with_outer_extractors() -> Result<(), Box<dyn Error>> {
    let portable = portable_result()?;
    assert_eq!(
        portable.schema_version,
        PORTABLE_ACCEPTED_RESULT_SCHEMA_VERSION
    );
    assert_eq!(portable.run_input.schema_version, RUN_INPUT_SCHEMA_VERSION);
    assert_eq!(
        portable.run_manifest.schema_version,
        RUN_MANIFEST_SCHEMA_VERSION
    );

    let bytes = serde_json::to_vec(&portable)?;
    assert_eq!(PortableAcceptedResultV2::from_json(&bytes)?, portable);
    let json = serde_json::to_value(&portable)?;
    assert_eq!(json["runInput"]["backendId"], json!("ortools.cp-sat"));
    assert!(json["runInput"].get("snapshotId").is_some());
    assert!(json["runInput"].get("scenarioSnapshotId").is_none());
    assert_eq!(extract_result_id(&json)?, portable.result_id.as_uuid());
    let dependency = extract_result_dependency(&json)?;
    assert_eq!(dependency.scenario_id, portable.scenario_id);
    assert_eq!(
        dependency.scenario_revision.value(),
        portable.scenario_revision
    );
    Ok(())
}

#[test]
// One mutation table keeps every immutable input field under the same strict parser contract.
#[allow(clippy::too_many_lines)]
fn run_input_rejects_checksum_version_hash_and_numeric_bounds() -> Result<(), Box<dyn Error>> {
    let accepted = accepted_contract()?;
    let input = run_input_for(
        &accepted,
        run_id(0x82)?,
        accepted.verification.document_hash.clone(),
        accepted.verification.planning_model_hash.clone(),
    )?;
    assert_eq!(RunInputV1::from_json(&serde_json::to_vec(&input)?)?, input);
    assert_eq!(
        input.request_semantics().canonical_hash()?,
        input.request_hash
    );
    let semantics = input.request_semantics();
    assert_eq!(
        semantics.schema_version,
        RUN_REQUEST_SEMANTICS_SCHEMA_VERSION
    );

    let mut invalid = input.clone();
    invalid.protocol_minor += 1;
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::RequestHashMismatch)
    );
    invalid = input.clone();
    invalid.solve_options.backend = BackendSelection::Specific(BackendId::new("other.backend")?);
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::RunInputBackendMismatch)
    );
    invalid = input.clone();
    invalid.solve_options.memory_limit_bytes = Some(0);
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::InvalidSolveOptions)
    );

    invalid = input.clone();
    invalid.schema_version = RUN_INPUT_SCHEMA_VERSION + 1;
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::UnsupportedVersion(2))
    );
    invalid = input.clone();
    invalid.scenario_revision = REVISION_MAX_V1 + 1;
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::LimitExceeded("scenario revision"))
    );
    invalid = input.clone();
    invalid.request_hash = "A".repeat(64);
    assert_eq!(invalid.validate(), Err(DomainContractError::InvalidBlake3));
    invalid = input.clone();
    invalid.temporary_condition_hash = Some("0".repeat(63));
    assert_eq!(invalid.validate(), Err(DomainContractError::InvalidBlake3));
    invalid = input.clone();
    invalid.compiler_version.clear();
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::InvalidRunVersion("compiler version"))
    );
    invalid = input.clone();
    invalid.solver_version = "x".repeat(MAX_RUN_VERSION_BYTES + 1);
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::InvalidRunVersion("solver version"))
    );
    invalid = input.clone();
    invalid.pack_schema_version = 0;
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::ZeroVersion("pack schema version"))
    );
    invalid = input.clone();
    invalid.planning_ir_schema_version = 0;
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::ZeroVersion(
            "planning IR schema version"
        ))
    );
    invalid = input.clone();
    invalid.protocol_major = 0;
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::ZeroVersion("protocol major"))
    );
    invalid = input.clone();
    invalid.checksum = blake3_hex(b"mutated-checksum");
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::ChecksumMismatch)
    );

    let mut unknown = serde_json::to_value(&input)?;
    unknown
        .as_object_mut()
        .ok_or("run input must serialize as object")?
        .insert("unknown".to_owned(), json!(true));
    assert!(matches!(
        RunInputV1::from_json(&serde_json::to_vec(&unknown)?),
        Err(DomainContractError::MalformedJson(_))
    ));
    assert!(matches!(
        RunInputV1::from_json(br#"{"schemaVersion":"one"}"#),
        Err(DomainContractError::MalformedJson(_))
    ));
    assert!(matches!(
        RunInputV1::from_json(br#"{"schemaVersion":1,"schemaVersion":1}"#),
        Err(DomainContractError::PortableJsonViolation(_))
    ));
    Ok(())
}

#[test]
// Every terminal status and timing partition stays together so the state matrix is auditable.
#[allow(clippy::too_many_lines)]
fn terminal_outcome_status_and_timing_partitions_are_strict() -> Result<(), Box<dyn Error>> {
    let accepted = accepted_contract()?;
    for status in [
        SolveStatus::Optimal,
        SolveStatus::Feasible,
        SolveStatus::Infeasible,
        SolveStatus::Unbounded,
        SolveStatus::NoSolutionWithinLimit,
        SolveStatus::Cancelled,
        SolveStatus::InvalidModel,
        SolveStatus::BackendUnavailable,
        SolveStatus::BackendFailed,
    ] {
        let outcome = RunTerminalOutcomeV1::Accepted {
            status,
            solution_id: accepted.solution.solution_id,
            accepted_result_checksum: accepted.checksum.clone(),
            verification_checksum: accepted.verification.checksum.clone(),
        };
        assert_eq!(
            outcome.validate().is_ok(),
            matches!(status, SolveStatus::Optimal | SolveStatus::Feasible)
        );
        let no_result = RunTerminalOutcomeV1::NoResult { status };
        assert_eq!(
            no_result.validate().is_ok(),
            !matches!(status, SolveStatus::Optimal | SolveStatus::Feasible)
        );
    }
    assert!(
        RunTerminalOutcomeV1::VerificationAlarm {
            diagnostic_code: "verification.score_mismatch".to_owned()
        }
        .validate()
        .is_ok()
    );
    for diagnostic_code in [
        "Verification.bad".to_owned(),
        "verification".to_owned(),
        format!("verification.{}", "x".repeat(MAX_RUN_DIAGNOSTIC_CODE_BYTES)),
    ] {
        assert_eq!(
            RunTerminalOutcomeV1::VerificationAlarm { diagnostic_code }.validate(),
            Err(DomainContractError::InvalidRunDiagnosticCode)
        );
    }

    let input = run_input_for(
        &accepted,
        run_id(0x82)?,
        accepted.verification.document_hash.clone(),
        accepted.verification.planning_model_hash.clone(),
    )?;
    assert!(matches!(
        manifest_for(
            input.run_id,
            input.checksum.clone(),
            accepted_outcome(&accepted),
            None,
            Vec::new(),
        ),
        Err(error) if error.downcast_ref::<DomainContractError>()
            == Some(&DomainContractError::InvalidRunTiming)
    ));
    let no_result = RunTerminalOutcomeV1::NoResult {
        status: SolveStatus::NoSolutionWithinLimit,
    };
    assert!(
        manifest_for(
            input.run_id,
            input.checksum.clone(),
            no_result.clone(),
            None,
            Vec::new(),
        )
        .is_ok()
    );
    assert!(matches!(
        manifest_for(
            input.run_id,
            input.checksum.clone(),
            no_result,
            Some(DurationMillis::ZERO),
            Vec::new(),
        ),
        Err(error) if error.downcast_ref::<DomainContractError>()
            == Some(&DomainContractError::InvalidRunTiming)
    ));
    let interrupted = RunManifestV1::new(
        input.run_id,
        input.checksum.clone(),
        RunTerminalOutcomeV1::Interrupted,
        "2026-09-03T12:00:01Z".parse()?,
        "2026-09-03T12:00:02Z".parse()?,
        None,
        None,
        None,
        RunPhaseTimingsV1::default(),
        Vec::new(),
    )?;
    assert!(interrupted.validate().is_ok());
    for invalid in [
        {
            let mut value = interrupted.clone();
            value.elapsed_milliseconds = Some(DurationMillis::ZERO);
            value
        },
        {
            let mut value = interrupted.clone();
            value.first_incumbent_milliseconds = Some(DurationMillis::ZERO);
            value
        },
        {
            let mut value = interrupted.clone();
            value.first_verified_feasible_milliseconds = Some(DurationMillis::ZERO);
            value
        },
        {
            let mut value = interrupted.clone();
            value.phase_timings.compile_milliseconds = Some(DurationMillis::ZERO);
            value
        },
        {
            let mut value = interrupted.clone();
            value.verification_warnings.push(warning("interrupted")?);
            value
        },
    ] {
        assert_eq!(
            invalid.validate(),
            Err(DomainContractError::InvalidRunTiming)
        );
    }
    assert_eq!(
        RunManifestV1::new(
            input.run_id,
            input.checksum.clone(),
            RunTerminalOutcomeV1::NoResult {
                status: SolveStatus::Cancelled,
            },
            "2026-09-03T12:00:01Z".parse()?,
            "2026-09-03T12:00:02Z".parse()?,
            None,
            None,
            None,
            RunPhaseTimingsV1::default(),
            Vec::new(),
        ),
        Err(DomainContractError::InvalidRunTiming)
    );
    assert!(matches!(
        manifest_for(
            input.run_id,
            input.checksum,
            RunTerminalOutcomeV1::Interrupted,
            None,
            vec![warning("bounded")?; MAX_VERIFICATION_WARNINGS + 1],
        ),
        Err(error) if error.downcast_ref::<DomainContractError>()
            == Some(&DomainContractError::LimitExceeded("verification warnings"))
    ));
    Ok(())
}

#[test]
// One parser fixture covers checksum, unknown-field, and numeric boundary rejection together.
#[allow(clippy::too_many_lines)]
fn run_manifest_rejects_checksum_unknown_fields_and_duration_bounds() -> Result<(), Box<dyn Error>>
{
    let portable = portable_result()?;
    let manifest = portable.run_manifest;
    assert_eq!(
        RunManifestV1::from_json(&serde_json::to_vec(&manifest)?)?,
        manifest
    );

    let mut invalid = manifest.clone();
    invalid.checksum = blake3_hex(b"other-manifest");
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::ChecksumMismatch)
    );
    invalid = manifest.clone();
    invalid.finished_at = "2026-09-03T11:59:59Z".parse()?;
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::InvalidRunTiming)
    );
    invalid = manifest.clone();
    invalid.first_incumbent_milliseconds = Some(DurationMillis::new(1_001)?);
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::InvalidRunTiming)
    );
    invalid = manifest.clone();
    invalid.first_verified_feasible_milliseconds = Some(DurationMillis::new(1_001)?);
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::InvalidRunTiming)
    );
    invalid = manifest.clone();
    invalid.first_incumbent_milliseconds = Some(DurationMillis::new(600)?);
    invalid.first_verified_feasible_milliseconds = Some(DurationMillis::new(500)?);
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::InvalidRunTiming)
    );
    invalid = manifest.clone();
    invalid.phase_timings = RunPhaseTimingsV1 {
        compile_milliseconds: Some(DurationMillis::new(600)?),
        backend_milliseconds: Some(DurationMillis::new(500)?),
        ..RunPhaseTimingsV1::default()
    };
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::InvalidRunTiming)
    );
    let over_elapsed = Some(DurationMillis::new(1_001)?);
    for phase_timings in [
        RunPhaseTimingsV1 {
            compile_milliseconds: over_elapsed,
            ..RunPhaseTimingsV1::default()
        },
        RunPhaseTimingsV1 {
            backend_milliseconds: over_elapsed,
            ..RunPhaseTimingsV1::default()
        },
        RunPhaseTimingsV1 {
            projection_milliseconds: over_elapsed,
            ..RunPhaseTimingsV1::default()
        },
        RunPhaseTimingsV1 {
            structural_validation_milliseconds: over_elapsed,
            ..RunPhaseTimingsV1::default()
        },
        RunPhaseTimingsV1 {
            score_recomputation_milliseconds: over_elapsed,
            ..RunPhaseTimingsV1::default()
        },
        RunPhaseTimingsV1 {
            required_rule_verification_milliseconds: over_elapsed,
            ..RunPhaseTimingsV1::default()
        },
        RunPhaseTimingsV1 {
            evidence_persistence_milliseconds: over_elapsed,
            ..RunPhaseTimingsV1::default()
        },
        RunPhaseTimingsV1 {
            optional_explanation_milliseconds: over_elapsed,
            ..RunPhaseTimingsV1::default()
        },
    ] {
        invalid = manifest.clone();
        invalid.phase_timings = phase_timings;
        assert_eq!(
            invalid.validate(),
            Err(DomainContractError::InvalidRunTiming)
        );
    }
    let mut unknown = serde_json::to_value(&manifest)?;
    unknown
        .as_object_mut()
        .ok_or("manifest must serialize as object")?
        .insert("nativeLog".to_owned(), json!("forbidden"));
    assert!(matches!(
        RunManifestV1::from_json(&serde_json::to_vec(&unknown)?),
        Err(DomainContractError::MalformedJson(_))
    ));
    let mut oversized = serde_json::to_value(&manifest)?;
    oversized["elapsedMilliseconds"] = json!(9_007_199_254_740_992_u64);
    assert!(matches!(
        RunManifestV1::from_json(&serde_json::to_vec(&oversized)?),
        Err(DomainContractError::MalformedJson(_))
    ));
    Ok(())
}

#[test]
// One mutation table proves every portable-result cross-binding independently rejects.
#[allow(clippy::too_many_lines)]
fn portable_result_rejects_every_cross_contract_binding() -> Result<(), Box<dyn Error>> {
    let portable = portable_result()?;
    let mut invalid = portable.clone();
    invalid.result_id = "01890a5d-ac96-7b64-9f74-bbfcf30f9f85".parse()?;
    assert!(matches!(
        invalid.validate(),
        Err(DomainContractError::PortableResultBindingMismatch(
            "result ID"
        ))
    ));
    invalid = portable.clone();
    invalid.scenario_id = "01890a5d-ac96-7b64-9f74-bbfcf30f9f86".parse()?;
    assert!(matches!(
        invalid.validate(),
        Err(DomainContractError::PortableResultBindingMismatch(
            "scenario ID"
        ))
    ));
    invalid = portable.clone();
    invalid.scenario_revision += 1;
    assert!(matches!(
        invalid.validate(),
        Err(DomainContractError::PortableResultBindingMismatch(
            "scenario revision"
        ))
    ));

    let accepted = portable.accepted_result.clone();
    let other_input = run_input_for(
        &accepted,
        run_id(0x85)?,
        accepted.verification.document_hash.clone(),
        accepted.verification.planning_model_hash.clone(),
    )?;
    invalid = portable.clone();
    invalid.run_manifest = accepted_manifest(&other_input, &accepted)?;
    assert!(matches!(
        invalid.validate(),
        Err(DomainContractError::PortableResultBindingMismatch("run ID"))
    ));
    invalid = portable.clone();
    invalid.run_manifest = manifest_for(
        invalid.run_input.run_id,
        blake3_hex(b"other-run-input"),
        accepted_outcome(&accepted),
        Some(DurationMillis::new(500)?),
        accepted.verification.warnings.clone(),
    )?;
    assert!(matches!(
        invalid.validate(),
        Err(DomainContractError::PortableResultBindingMismatch(
            "run-input checksum"
        ))
    ));

    let bad_outcomes = [
        RunTerminalOutcomeV1::Accepted {
            status: SolveStatus::Feasible,
            solution_id: "01890a5d-ac96-7b64-9f74-bbfcf30f9f85".parse()?,
            accepted_result_checksum: accepted.checksum.clone(),
            verification_checksum: accepted.verification.checksum.clone(),
        },
        RunTerminalOutcomeV1::Accepted {
            status: SolveStatus::Feasible,
            solution_id: accepted.solution.solution_id,
            accepted_result_checksum: blake3_hex(b"other-result"),
            verification_checksum: accepted.verification.checksum.clone(),
        },
        RunTerminalOutcomeV1::Accepted {
            status: SolveStatus::Feasible,
            solution_id: accepted.solution.solution_id,
            accepted_result_checksum: accepted.checksum.clone(),
            verification_checksum: blake3_hex(b"other-report"),
        },
    ];
    for outcome in bad_outcomes {
        invalid = portable.clone();
        invalid.run_manifest = manifest_for(
            invalid.run_input.run_id,
            invalid.run_input.checksum.clone(),
            outcome,
            Some(DurationMillis::new(500)?),
            accepted.verification.warnings.clone(),
        )?;
        assert!(matches!(
            invalid.validate(),
            Err(DomainContractError::PortableResultBindingMismatch(
                "accepted outcome"
            ))
        ));
    }
    invalid = portable.clone();
    invalid.run_manifest = manifest_for(
        invalid.run_input.run_id,
        invalid.run_input.checksum.clone(),
        RunTerminalOutcomeV1::NoResult {
            status: SolveStatus::Infeasible,
        },
        None,
        accepted.verification.warnings.clone(),
    )?;
    assert!(matches!(
        invalid.validate(),
        Err(DomainContractError::PortableResultBindingMismatch(
            "terminal outcome"
        ))
    ));
    invalid = portable.clone();
    invalid.run_manifest = manifest_for(
        invalid.run_input.run_id,
        invalid.run_input.checksum.clone(),
        accepted_outcome(&accepted),
        Some(DurationMillis::new(500)?),
        Vec::new(),
    )?;
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::PortableResultBindingMismatch(
            "verification warnings"
        ))
    );

    let late_snapshot_input = run_input_at(
        &accepted,
        portable.run_input.run_id,
        accepted.verification.document_hash.clone(),
        accepted.verification.planning_model_hash.clone(),
        "2026-09-03T12:00:02Z".parse()?,
    )?;
    invalid = portable.clone();
    invalid.run_manifest = accepted_manifest(&late_snapshot_input, &accepted)?;
    invalid.run_input = late_snapshot_input;
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::PortableResultBindingMismatch(
            "snapshot creation time"
        ))
    );
    Ok(())
}

#[test]
fn portable_result_binds_pack_document_model_and_nested_checksums() -> Result<(), Box<dyn Error>> {
    let portable = portable_result()?;
    let accepted = portable.accepted_result.clone();

    for (document_hash, model_hash, expected) in [
        (
            blake3_hex(b"other-document"),
            accepted.verification.planning_model_hash.clone(),
            "snapshot document hash",
        ),
        (
            accepted.verification.document_hash.clone(),
            blake3_hex(b"other-model"),
            "planning model hash",
        ),
    ] {
        let input = run_input_for(
            &accepted,
            portable.run_input.run_id,
            document_hash,
            model_hash,
        )?;
        let manifest = accepted_manifest(&input, &accepted)?;
        let invalid = PortableAcceptedResultV2 {
            run_input: input,
            run_manifest: manifest,
            ..portable.clone()
        };
        assert_eq!(
            invalid.validate(),
            Err(DomainContractError::PortableResultBindingMismatch(expected))
        );
    }

    let mut other_solution = accepted.solution.clone();
    other_solution.pack_id = PackId::new("official.other")?;
    let other_report = report(&other_solution, 0, true)?;
    let other_accepted = AcceptedResult::new(other_solution, other_report)?;
    let manifest = manifest_for(
        portable.run_input.run_id,
        portable.run_input.checksum.clone(),
        accepted_outcome(&other_accepted),
        Some(DurationMillis::new(500)?),
        other_accepted.verification.warnings.clone(),
    )?;
    let invalid = PortableAcceptedResultV2 {
        accepted_result: other_accepted,
        run_manifest: manifest,
        ..portable.clone()
    };
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::PortableResultBindingMismatch(
            "pack ID"
        ))
    );

    let mut invalid = portable.clone();
    invalid.run_input.checksum = blake3_hex(b"bad-input-checksum");
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::ChecksumMismatch)
    );
    invalid = portable.clone();
    invalid.run_manifest.checksum = blake3_hex(b"bad-manifest-checksum");
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::ChecksumMismatch)
    );
    invalid = portable.clone();
    invalid.evidence.insert(
        DomainEvidenceId::new("evidence.first")?,
        VerificationValue::Integer(2),
    );
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::ChecksumMismatch)
    );
    invalid = portable;
    invalid.checksum = blake3_hex(b"bad-portable-checksum");
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::ChecksumMismatch)
    );
    Ok(())
}

#[test]
fn portable_result_rejects_newer_unknown_and_evidence_bounds() -> Result<(), Box<dyn Error>> {
    let portable = portable_result()?;
    let mut invalid = portable.clone();
    invalid.schema_version = PORTABLE_ACCEPTED_RESULT_SCHEMA_VERSION + 1;
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::UnsupportedVersion(3))
    );

    let mut unknown = serde_json::to_value(&portable)?;
    unknown
        .as_object_mut()
        .ok_or("portable result must serialize as object")?
        .insert("backendAuthority".to_owned(), json!(true));
    assert!(matches!(
        PortableAcceptedResultV2::from_json(&serde_json::to_vec(&unknown)?),
        Err(DomainContractError::MalformedJson(_))
    ));

    invalid = portable.clone();
    invalid.evidence.insert(
        DomainEvidenceId::new("evidence.text")?,
        VerificationValue::Text("x".repeat(MAX_VERIFICATION_TEXT_BYTES + 1)),
    );
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::InvalidVerificationText)
    );

    invalid = portable.clone();
    invalid.evidence.insert(
        DomainEvidenceId::new("evidence.orphan")?,
        VerificationValue::Integer(1),
    );
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::PortableResultBindingMismatch(
            "evidence reference"
        ))
    );

    invalid = portable;
    invalid.evidence.clear();
    for index in 0..=MAX_PORTABLE_RESULT_EVIDENCE_RECORDS {
        invalid.evidence.insert(
            DomainEvidenceId::new(format!("evidence.item-{index}"))?,
            VerificationValue::Integer(0),
        );
    }
    assert_eq!(
        invalid.validate(),
        Err(DomainContractError::LimitExceeded(
            "portable result evidence"
        ))
    );
    Ok(())
}
#[test]
fn every_json_entrypoint_rejects_oversized_input_before_deserialization() {
    let bytes = vec![b' '; MAX_DOMAIN_CONTRACT_JSON_BYTES + 1];
    macro_rules! assert_oversized {
        ($contract:ty) => {
            assert_eq!(
                <$contract>::from_json(&bytes),
                Err(DomainContractError::LimitExceeded(
                    "domain contract JSON bytes"
                ))
            );
        };
    }
    assert_oversized!(NormalizedSolution);
    assert_oversized!(VerificationScope);
    assert_oversized!(VerificationContextV1);
    assert_oversized!(VerificationReport);
    assert_oversized!(AcceptedResult);
    assert_oversized!(RunInputV1);
    assert_oversized!(RunManifestV1);
    assert_oversized!(PortableAcceptedResultV2);
    assert_oversized!(LegacyVerificationReportV1);
    assert_oversized!(LegacyAcceptedResultV1);
}

#[test]
fn portable_ingress_applies_shared_nonsecret_policy_before_serde() -> Result<(), Box<dyn Error>> {
    let portable = portable_result()?;
    let mut input = serde_json::to_value(&portable.run_input)?;
    input
        .as_object_mut()
        .ok_or("run input must serialize as object")?
        .insert("secretToken".to_owned(), json!("not-retained"));
    assert!(matches!(
        RunInputV1::from_json(&serde_json::to_vec(&input)?),
        Err(DomainContractError::PortableJsonViolation(_))
    ));

    let mut accepted = serde_json::to_value(&portable.accepted_result)?;
    accepted["verification"]["warnings"][0]["facts"]["fact.detail"] = json!("/tmp/native.log");
    assert!(matches!(
        AcceptedResult::from_json(&serde_json::to_vec(&accepted)?),
        Err(DomainContractError::PortableJsonViolation(_))
    ));
    let deep = format!(
        "{}0{}",
        "[".repeat(MAX_DOMAIN_CONTRACT_JSON_DEPTH + 1),
        "]".repeat(MAX_DOMAIN_CONTRACT_JSON_DEPTH + 1)
    );
    assert!(matches!(
        NormalizedSolution::from_json(deep.as_bytes()),
        Err(DomainContractError::PortableJsonViolation(_))
    ));

    let long_string = serde_json::to_vec(&"x".repeat(MAX_VERIFICATION_TEXT_BYTES + 1))?;
    assert!(matches!(
        NormalizedSolution::from_json(&long_string),
        Err(DomainContractError::PortableJsonViolation(_))
    ));

    let too_many_items = format!("[{}0]", "0,".repeat(MAX_DOMAIN_CONTRACT_JSON_ITEMS));
    assert!(matches!(
        NormalizedSolution::from_json(too_many_items.as_bytes()),
        Err(DomainContractError::PortableJsonViolation(_))
    ));
    Ok(())
}

fn mixed_score(
    feasibility: i64,
    minimized: i64,
    maximized: i64,
) -> Result<ScoreVector, Box<dyn Error>> {
    Ok(ScoreVector {
        feasibility,
        levels: vec![
            ScoreLevelValue {
                level_id: ScoreLevelId::new("score.primary")?,
                value: minimized,
                direction: OptimizationDirection::Minimize,
                category_breakdown: BTreeMap::new(),
            },
            ScoreLevelValue {
                level_id: ScoreLevelId::new("score.secondary")?,
                value: maximized,
                direction: OptimizationDirection::Maximize,
                category_breakdown: BTreeMap::new(),
            },
        ],
    })
}

#[test]
fn score_shape_is_bounded_and_category_breakdowns_are_optional() -> Result<(), Box<dyn Error>> {
    let mut invalid = score(0)?;
    invalid.feasibility = -1;
    assert_eq!(
        invalid.validate_shape(),
        Err(DomainContractError::NegativeFeasibility)
    );

    let level = score(0)?.levels.remove(0);
    let mut too_many_levels = ScoreVector {
        feasibility: 0,
        levels: vec![level.clone(); MAX_SCORE_LEVELS + 1],
    };
    for (index, score_level) in too_many_levels.levels.iter_mut().enumerate() {
        score_level.level_id = ScoreLevelId::new(format!("score.level.{index}"))?;
    }
    assert_eq!(
        too_many_levels.validate_current_shape(),
        Err(DomainContractError::LimitExceeded("score levels"))
    );
    assert_eq!(
        too_many_levels.compare(&too_many_levels),
        Err(DomainContractError::LimitExceeded("score levels"))
    );

    let mut categories = BTreeMap::new();
    for index in 0..MAX_SCORE_CATEGORIES_PER_LEVEL {
        categories.insert(
            ScoreCategoryId::new(format!("score.category.{index}"))?,
            i64::try_from(index)?,
        );
    }
    let mut bounded = score(7)?;
    bounded.levels[0].category_breakdown = categories;
    assert!(bounded.validate_current_shape().is_ok());
    bounded.levels[0]
        .category_breakdown
        .insert(ScoreCategoryId::new("score.category.overflow")?, -1);
    assert_eq!(
        bounded.validate_current_shape(),
        Err(DomainContractError::LimitExceeded("score categories"))
    );
    assert_eq!(
        bounded.compare(&bounded),
        Err(DomainContractError::LimitExceeded("score categories"))
    );

    let mut duplicate = mixed_score(0, 1, 2)?;
    duplicate.levels[1].level_id = duplicate.levels[0].level_id.clone();
    assert_eq!(
        duplicate.validate_shape(),
        Err(DomainContractError::DuplicateScoreLevel)
    );
    assert!(score(7)?.validate_shape().is_ok());
    Ok(())
}

#[test]
fn score_uses_feasibility_then_mixed_lexicographic_direction() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        mixed_score(0, 2, i64::MIN)?.compare(&mixed_score(0, 3, i64::MAX)?)?,
        Ordering::Less
    );
    assert_eq!(
        mixed_score(0, 2, 5)?.compare(&mixed_score(0, 2, 4)?)?,
        Ordering::Less
    );
    assert_eq!(
        mixed_score(1, i64::MIN, i64::MAX)?.compare(&mixed_score(0, i64::MAX, i64::MIN)?)?,
        Ordering::Greater
    );

    let mut mismatch = mixed_score(0, 2, 5)?;
    mismatch.levels[1].direction = OptimizationDirection::Minimize;
    assert_eq!(
        mixed_score(0, 2, 5)?.compare(&mismatch),
        Err(DomainContractError::ScoreShapeMismatch)
    );
    Ok(())
}

#[test]
fn score_serialization_is_deterministic_and_breakdowns_need_not_sum() -> Result<(), Box<dyn Error>>
{
    let first = ScoreCategoryId::new("score.category.first")?;
    let second = ScoreCategoryId::new("score.category.second")?;
    let mut left = score(42)?;
    left.levels[0].category_breakdown = BTreeMap::from([(second.clone(), -7), (first.clone(), 99)]);
    let mut right = score(42)?;
    right.levels[0].category_breakdown = BTreeMap::from([(first, 99), (second, -7)]);

    assert!(left.validate_shape().is_ok());
    assert_eq!(left.compare(&right)?, Ordering::Equal);
    assert_eq!(serde_json::to_vec(&left)?, serde_json::to_vec(&right)?);
    Ok(())
}

proptest! {
    #[test]
    fn mixed_direction_score_comparison_is_a_total_preorder(
        left in (0_i64..100, any::<i64>(), any::<i64>()),
        middle in (0_i64..100, any::<i64>(), any::<i64>()),
        right in (0_i64..100, any::<i64>(), any::<i64>()),
    ) {
        let Ok(left) = mixed_score(left.0, left.1, left.2) else {
            return Ok(());
        };
        let Ok(middle) = mixed_score(middle.0, middle.1, middle.2) else {
            return Ok(());
        };
        let Ok(right) = mixed_score(right.0, right.1, right.2) else {
            return Ok(());
        };

        prop_assert_eq!(left.compare(&left), Ok(Ordering::Equal));
        let Ok(left_middle) = left.compare(&middle) else {
            return Ok(());
        };
        let Ok(middle_left) = middle.compare(&left) else {
            return Ok(());
        };
        let Ok(middle_right) = middle.compare(&right) else {
            return Ok(());
        };
        prop_assert_eq!(left_middle, middle_left.reverse());
        if left_middle != Ordering::Greater && middle_right != Ordering::Greater {
            prop_assert_ne!(left.compare(&right), Ok(Ordering::Greater));
        }
        if left_middle == Ordering::Equal {
            prop_assert_eq!(left.feasibility, middle.feasibility);
            prop_assert!(left
                .levels
                .iter()
                .zip(&middle.levels)
                .all(|(left, right)| left.value == right.value));
        }
    }
}
