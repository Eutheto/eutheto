#![no_main]

#[path = "../support.rs"]
mod support;

use eutheto_domain_ir::DomainAssignmentId;
use eutheto_planning_ir::*;
use libfuzzer_sys::fuzz_target;
use support::{LIMITS, entity, int_id, problem_with_variables, provenance_id};

const MAX_RANGES: usize = 16;

fn word(data: &[u8], offset: usize) -> i64 {
    let mut bytes = [0_u8; 8];
    if offset < data.len() {
        let available = (data.len() - offset).min(bytes.len());
        bytes[..available].copy_from_slice(&data[offset..offset + available]);
    }
    i64::from_le_bytes(bytes)
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 512 {
        return;
    }

    let range_count = data
        .first()
        .map_or(1, |value| usize::from(*value % MAX_RANGES as u8) + 1);
    let ranges: Vec<_> = (0..range_count)
        .map(|index| InclusiveRange {
            start: word(data, 1 + index * 16),
            end: word(data, 9 + index * 16),
        })
        .collect();
    if let Ok(domain) = IntDomain::new(ranges.clone()) {
        assert!(domain.inclusive_ranges.len() <= ranges.len());
        assert!(domain
            .inclusive_ranges
            .windows(2)
            .all(|pair| pair[0]
                .end
                .checked_add(1)
                .is_some_and(|next| next < pair[1].start)));
        for range in &ranges {
            let middle = ((i128::from(range.start) + i128::from(range.end)) / 2) as i64;
            for value in [range.start, middle, range.end] {
                assert!(domain.contains(value));
            }
        }
        for probe_index in 0..4 {
            let probe = word(data, 257 + probe_index * 8);
            assert_eq!(
                domain.contains(probe),
                ranges
                    .iter()
                    .any(|range| range.start <= probe && probe <= range.end)
            );
        }
        let mut reversed = ranges;
        reversed.reverse();
        assert_eq!(IntDomain::new(reversed), Ok(domain));
    }

    let coefficients = [
        word(data, 0),
        word(data, 8),
        word(data, 16),
        word(data, 24),
    ];
    let constant = word(data, 32);
    let terms = vec![
        LinearTerm {
            variable: int_id("x"),
            coefficient: coefficients[0],
        },
        LinearTerm {
            variable: int_id("y"),
            coefficient: coefficients[1],
        },
        LinearTerm {
            variable: int_id("x"),
            coefficient: coefficients[2],
        },
        LinearTerm {
            variable: int_id("y"),
            coefficient: coefficients[3],
        },
    ];
    let Ok(expression) = LinearExpression::new(terms.clone(), constant) else {
        return;
    };
    assert!(expression
        .terms
        .windows(2)
        .all(|pair| pair[0].variable < pair[1].variable));
    assert!(expression.terms.iter().all(|term| term.coefficient != 0));

    let x_sum = i128::from(coefficients[0]) + i128::from(coefficients[2]);
    let y_sum = i128::from(coefficients[1]) + i128::from(coefficients[3]);
    for (id, expected) in [(int_id("x"), x_sum), (int_id("y"), y_sum)] {
        let actual = expression
            .terms
            .iter()
            .find(|term| term.variable == id)
            .map_or(0, |term| i128::from(term.coefficient));
        assert_eq!(actual, expected);
    }
    let mut reversed = terms;
    reversed.reverse();
    if let Ok(reverse_expression) = LinearExpression::new(reversed, constant) {
        assert_eq!(expression, reverse_expression);
    }

    let mut problem = problem_with_variables(false);
    problem.projections.push(SolutionProjection {
        id: ProjectionId::new("projection.expression").expect("fixed projection ID is canonical"),
        assignment_id: DomainAssignmentId::new("assignment.expression")
            .expect("fixed assignment ID is canonical"),
        entity: entity(),
        required: false,
        expression: ProjectionExpression::Linear(expression.clone()),
        provenance: provenance_id(),
    });
    problem
        .canonicalize()
        .expect("normalized expression remains canonicalizable");

    let first = validate(&problem, LIMITS);
    let second = validate(&problem, LIMITS);
    assert_eq!(first, second, "checked expression bounds must be deterministic");
    let expected_valid = expression.terms.iter().all(|term| {
        term.coefficient.unsigned_abs() <= LIMITS.max_abs_coefficient as u64
    }) && constant.unsigned_abs() <= LIMITS.max_abs_value as u64;
    assert_eq!(first.is_ok(), expected_valid);
});
