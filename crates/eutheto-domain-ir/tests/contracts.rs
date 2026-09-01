use eutheto_domain_ir::*;
use eutheto_types::{PackId, ScenarioId, SolutionId};
use proptest::prelude::*;
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

#[test]
fn ids_accept_required_grammar_and_boundaries() -> Result<(), Box<dyn Error>> {
    let id = DomainAssignmentId::new("school_2.section-a")?;
    assert_eq!(id.as_str(), "school_2.section-a");
    assert!(DomainAssignmentId::new("School.section").is_err());
    assert!(DomainAssignmentId::new("school").is_err());
    assert!(DomainAssignmentId::new(format!("aa.{}", "b".repeat(158))).is_err());
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
fn normalized_solution_requires_canonical_ids_and_evidence() -> Result<(), Box<dyn Error>> {
    let mut value = solution(3)?;
    let entity = DomainEntityRef {
        kind: DomainEntityKindId::new("school.section")?,
        id: DomainEntityId::new("school.section-a")?,
    };
    let assignment = DomainAssignment {
        id: DomainAssignmentId::new("school.second")?,
        entity: entity.clone(),
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
    assert!(value.validate().is_ok());
    let duplicate = value.assignments[0].clone();
    value.assignments.push(duplicate);
    assert!(matches!(
        value.canonicalize(),
        Err(DomainContractError::DuplicateAssignment(_))
    ));
    Ok(())
}

#[test]
fn score_uses_feasibility_then_stable_lexicographic_direction() -> Result<(), Box<dyn Error>> {
    assert_eq!(score(2)?.compare(&score(3)?)?, Ordering::Less);
    let mut maximized = score(2)?;
    maximized.levels[0].direction = OptimizationDirection::Maximize;
    let mut other = score(3)?;
    other.levels[0].direction = OptimizationDirection::Maximize;
    assert_eq!(maximized.compare(&other)?, Ordering::Greater);
    let mut infeasible = score(0)?;
    infeasible.feasibility = 1;
    assert_eq!(infeasible.compare(&score(i64::MAX)?)?, Ordering::Greater);
    let mut mismatch = score(2)?;
    mismatch.levels[0].level_id = ScoreLevelId::new("score.other")?;
    assert_eq!(
        score(2)?.compare(&mismatch),
        Err(DomainContractError::ScoreShapeMismatch)
    );
    Ok(())
}

#[test]
fn accepted_result_is_a_strict_data_gate() -> Result<(), Box<dyn Error>> {
    let report = VerificationReport {
        schema_version: VERIFICATION_REPORT_SCHEMA_VERSION,
        scenario_revision: 7,
        feasible: true,
        issues: Vec::new(),
        score: Some(score(4)?),
    };
    assert!(AcceptedResult::new(solution(7)?, report.clone()).is_ok());
    assert_eq!(
        AcceptedResult::new(solution(8)?, report.clone()),
        Err(DomainContractError::RevisionMismatch)
    );
    let mut no_score = report;
    no_score.score = None;
    assert_eq!(
        AcceptedResult::new(solution(7)?, no_score),
        Err(DomainContractError::MissingVerifiedScore)
    );
    Ok(())
}

#[test]
fn strict_json_rejects_unknown_field_and_version() -> Result<(), Box<dyn Error>> {
    let value = solution(1)?;
    let mut json = serde_json::to_value(&value)?;
    if let Some(object) = json.as_object_mut() {
        object.insert("futureField".to_owned(), serde_json::Value::Bool(true));
    }
    let bytes = serde_json::to_vec(&json)?;
    assert!(matches!(
        NormalizedSolution::from_json(&bytes),
        Err(DomainContractError::MalformedJson(_))
    ));
    let mut future = value;
    future.schema_version = 2;
    let bytes = serde_json::to_vec(&future)?;
    assert_eq!(
        NormalizedSolution::from_json(&bytes),
        Err(DomainContractError::UnsupportedVersion(2))
    );
    Ok(())
}

proptest! {
    #[test]
    fn score_comparison_is_transitive(left in any::<i64>(), middle in any::<i64>(), right in any::<i64>()) {
        let Ok(left_score) = score(left) else { return Ok(()); };
        let Ok(middle_score) = score(middle) else { return Ok(()); };
        let Ok(right_score) = score(right) else { return Ok(()); };
        let Ok(left_middle) = left_score.compare(&middle_score) else { return Ok(()); };
        let Ok(middle_right) = middle_score.compare(&right_score) else { return Ok(()); };
        if left_middle != Ordering::Greater && middle_right != Ordering::Greater {
            prop_assert_ne!(left_score.compare(&right_score), Ok(Ordering::Greater));
        }
    }
}
