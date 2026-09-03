use eutheto_domain_ir::*;
use eutheto_types::{PackId, REVISION_MAX_V1, RuleId, ScenarioId, SolutionId};
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
