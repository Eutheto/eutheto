#![no_main]

#[path = "../support.rs"]
mod support;

use eutheto_domain_ir::DomainAssignmentId;
use eutheto_planning_ir::*;
use libfuzzer_sys::fuzz_target;
use support::{LIMITS, entity, int_id, problem_with_variables, provenance_id};

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 {
        return;
    }

    let flags = data.first().copied().unwrap_or(0);
    let constraint_count = usize::from(data.get(1).copied().unwrap_or(0) % 8);
    let mut problem = problem_with_variables(false);
    if flags & 1 != 0 {
        problem.projections.push(SolutionProjection {
            id: ProjectionId::new("projection.link").expect("fixed projection ID is canonical"),
            assignment_id: DomainAssignmentId::new("assignment.link")
                .expect("fixed assignment ID is canonical"),
            entity: entity(),
            required: true,
            expression: ProjectionExpression::Linear(
                LinearExpression::new(
                    vec![
                        LinearTerm {
                            variable: int_id("x"),
                            coefficient: 1,
                        },
                        LinearTerm {
                            variable: int_id("y"),
                            coefficient: if flags & 2 == 0 { 1 } else { -1 },
                        },
                    ],
                    0,
                )
                .expect("fixed linking expression is valid"),
            ),
            provenance: provenance_id(),
        });
    }
    for index in 0..constraint_count {
        problem.constraints.push(ConstraintRecord {
            id: PlanningConstraintId::new(format!("constraint.link-{index}"))
                .expect("bounded generated constraint ID is canonical"),
            body: Constraint::Equality {
                left: int_id("x"),
                right: int_id("y"),
            },
            enforcement: Vec::new(),
            provenance: provenance_id(),
            tags: Vec::new(),
        });
    }
    problem
        .canonicalize()
        .expect("bounded component problem canonicalizes");
    assert!(validate(&problem, LIMITS).is_ok());

    let first = analyze_components(&problem);
    let second = analyze_components(&problem);
    assert_eq!(first, second, "component analysis must be deterministic");
    let linked = flags & 1 != 0 || constraint_count != 0;
    assert_eq!(first.components.len(), if linked { 1 } else { 2 });
    assert_eq!(first.edge_count, u64::from(flags & 1 != 0) + constraint_count as u64);

    problem.split_authorization = Some(SplitAuthorization {
        component_hash: "wrong".to_owned(),
        domain_merge_contract: "fuzz.merge-v1".to_owned(),
        projection_independent: true,
    });
    assert!(matches!(
        validate(&problem, LIMITS),
        Err(ValidationError {
            code: ValidationCode::InvalidSplitAuthorization,
            ..
        })
    ));

    problem.split_authorization = Some(SplitAuthorization {
        component_hash: first.component_hash.clone(),
        domain_merge_contract: "fuzz.merge-v1".to_owned(),
        projection_independent: true,
    });
    assert!(validate(&problem, LIMITS).is_ok());
    assert_eq!(analyze_components(&problem), first);

    problem
        .split_authorization
        .as_mut()
        .expect("authorization is present")
        .projection_independent = false;
    assert!(matches!(
        validate(&problem, LIMITS),
        Err(ValidationError {
            code: ValidationCode::InvalidSplitAuthorization,
            ..
        })
    ));
});
