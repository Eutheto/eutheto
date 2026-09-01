#![no_main]

#[path = "../support.rs"]
mod support;

use eutheto_domain_ir::DomainAssignmentId;
use eutheto_planning_ir::*;
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeMap;
use support::{
    LIMITS, bool_id, entity, int_id, problem_with_variables, provenance_id, solution_id,
};

fn candidate_value(data: &[u8]) -> i64 {
    let low = data.get(1).copied().unwrap_or(0);
    let high = data.get(2).copied().unwrap_or(0);
    i64::from(i16::from_le_bytes([low, high]))
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 {
        return;
    }

    let mut problem = problem_with_variables(true);
    problem.projections = vec![
        SolutionProjection {
            id: ProjectionId::new("projection.enabled")
                .expect("fixed projection ID is canonical"),
            assignment_id: DomainAssignmentId::new("assignment.enabled")
                .expect("fixed assignment ID is canonical"),
            entity: entity(),
            required: false,
            expression: ProjectionExpression::Boolean(bool_id("enabled")),
            provenance: provenance_id(),
        },
        SolutionProjection {
            id: ProjectionId::new("projection.x").expect("fixed projection ID is canonical"),
            assignment_id: DomainAssignmentId::new("assignment.x")
                .expect("fixed assignment ID is canonical"),
            entity: entity(),
            required: true,
            expression: ProjectionExpression::Integer(int_id("x")),
            provenance: provenance_id(),
        },
    ];
    problem
        .canonicalize()
        .expect("fixed projection problem canonicalizes");
    assert!(validate(&problem, LIMITS).is_ok());

    let flags = data.first().copied().unwrap_or(0);
    let mut candidate = CandidateValues::default();
    if flags & 1 != 0 {
        candidate.booleans.insert(bool_id("enabled"), flags & 8 != 0);
    }
    if flags & 2 != 0 {
        candidate
            .integers
            .insert(int_id("x"), candidate_value(data));
    }
    if flags & 4 != 0 {
        candidate
            .integers
            .insert(int_id("y"), -candidate_value(data));
    }

    let first = project_candidate(&problem, &candidate, solution_id(), LIMITS);
    let second = project_candidate(&problem, &candidate, solution_id(), LIMITS);
    match (first, second) {
        (Ok(first), Ok(second)) => {
            assert_eq!(first, second);
            assert!(first
                .assignments
                .windows(2)
                .all(|pair| pair[0].id < pair[1].id));
        }
        (Err(first), Err(second)) => {
            assert_eq!(format!("{first:?}"), format!("{second:?}"));
        }
        _ => panic!("projection acceptance must be deterministic"),
    }

    let complete = CandidateValues {
        booleans: BTreeMap::from([(bool_id("enabled"), true)]),
        integers: BTreeMap::from([(int_id("x"), 0)]),
    };
    let mut unknown = complete.clone();
    unknown.integers.insert(
        IntVariableId::new("int.unknown").expect("fixed unknown ID is canonical"),
        0,
    );
    assert!(matches!(
        project_candidate(&problem, &unknown, solution_id(), LIMITS),
        Err(ProjectionError::UnknownCandidateValue(_))
    ));

    let mut out_of_domain = complete;
    out_of_domain.integers.insert(int_id("x"), 1_001);
    assert!(matches!(
        project_candidate(&problem, &out_of_domain, solution_id(), LIMITS),
        Err(ProjectionError::OutOfDomain(id)) if id == int_id("x")
    ));
});
