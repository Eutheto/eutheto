//! Deterministic small-assignment evaluator used only by this crate's tests.

use crate::{
    CandidateValues, ComparisonOp, Constraint, ConstraintRecord, IntervalVariable, Literal,
    PlanningProblem, ProjectionError, Variable,
};
use std::collections::BTreeMap;

pub(crate) fn evaluate_record(
    record: &ConstraintRecord,
    problem: &PlanningProblem,
    candidate: &CandidateValues,
) -> Result<bool, ProjectionError> {
    for literal in &record.enforcement {
        if !literal_value(literal, candidate)? {
            return Ok(true);
        }
    }
    evaluate_constraint(&record.body, problem, candidate)
}

// Keeping every constraint variant in one exhaustive evaluator makes test semantics auditable.
#[allow(clippy::too_many_lines)]
fn evaluate_constraint(
    constraint: &Constraint,
    problem: &PlanningProblem,
    candidate: &CandidateValues,
) -> Result<bool, ProjectionError> {
    match constraint {
        Constraint::BoolOr { literals } => any_literals(literals, candidate),
        Constraint::BoolAnd { literals } => all_literals(literals, candidate),
        Constraint::Implication {
            antecedent,
            consequent,
        } => Ok(!literal_value(antecedent, candidate)? || literal_value(consequent, candidate)?),
        Constraint::Equivalence { left, right } => {
            Ok(literal_value(left, candidate)? == literal_value(right, candidate)?)
        }
        Constraint::AtMostOne { literals } => Ok(true_count(literals, candidate)? <= 1),
        Constraint::ExactlyOne { literals } => Ok(true_count(literals, candidate)? == 1),
        Constraint::CardinalityRange { literals, min, max } => {
            let count = true_count(literals, candidate)?;
            Ok(*min <= count && count <= *max)
        }
        Constraint::LinearComparison(comparison) => {
            let value = linear_value(&comparison.expression, candidate)?;
            Ok(compare(value, comparison.op, comparison.rhs))
        }
        Constraint::ReifiedLinearComparison {
            literal,
            comparison,
        } => {
            let value = linear_value(&comparison.expression, candidate)?;
            Ok(literal_value(literal, candidate)? == compare(value, comparison.op, comparison.rhs))
        }
        Constraint::AllDifferent { variables } => {
            let mut values = Vec::with_capacity(variables.len());
            for variable in variables {
                values.push(integer(variable, candidate)?);
            }
            values.sort_unstable();
            Ok(values.windows(2).all(|pair| pair[0] != pair[1]))
        }
        Constraint::AllowedTable { variables, rows } => {
            let row = row_values(variables, candidate)?;
            Ok(rows.binary_search(&row).is_ok())
        }
        Constraint::ForbiddenTable { variables, rows } => {
            let row = row_values(variables, candidate)?;
            Ok(rows.binary_search(&row).is_err())
        }
        Constraint::Element {
            index,
            values,
            target,
        } => {
            let index = integer(index, candidate)?;
            let target = integer(target, candidate)?;
            let Ok(index) = usize::try_from(index) else {
                return Ok(false);
            };
            Ok(values.get(index).is_some_and(|value| *value == target))
        }
        Constraint::Min { target, inputs } => {
            let target = integer(target, candidate)?;
            let mut values = Vec::with_capacity(inputs.len());
            for input in inputs {
                values.push(integer(input, candidate)?);
            }
            Ok(values
                .into_iter()
                .min()
                .is_some_and(|value| value == target))
        }
        Constraint::Max { target, inputs } => {
            let target = integer(target, candidate)?;
            let mut values = Vec::with_capacity(inputs.len());
            for input in inputs {
                values.push(integer(input, candidate)?);
            }
            Ok(values
                .into_iter()
                .max()
                .is_some_and(|value| value == target))
        }
        Constraint::Equality { left, right } => {
            Ok(integer(left, candidate)? == integer(right, candidate)?)
        }
        Constraint::AbsDifference {
            target,
            left,
            right,
        } => {
            let difference = integer(left, candidate)?
                .checked_sub(integer(right, candidate)?)
                .and_then(i64::checked_abs)
                .ok_or(ProjectionError::ArithmeticOverflow)?;
            Ok(integer(target, candidate)? == difference)
        }
        Constraint::NoOverlap { intervals } => {
            let declared = intervals_by_id(problem);
            let mut present = Vec::new();
            for id in intervals {
                if let Some(interval) = present_interval(declared.get(id).copied(), candidate)? {
                    present.push(interval);
                }
            }
            for left in 0..present.len() {
                for right in (left + 1)..present.len() {
                    if present[left].0 < present[right].1 && present[right].0 < present[left].1 {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        }
        Constraint::Cumulative {
            intervals,
            demands,
            capacity,
        } => {
            let declared = intervals_by_id(problem);
            let mut present = Vec::new();
            for (id, demand) in intervals.iter().zip(demands) {
                if let Some((start, end)) = present_interval(declared.get(id).copied(), candidate)?
                {
                    present.push((start, end, *demand));
                }
            }
            let instants: Vec<_> = present.iter().map(|(start, _, _)| *start).collect();
            for instant in instants {
                let mut demand = 0_i64;
                for (start, end, value) in &present {
                    if *start <= instant && instant < *end {
                        demand = demand
                            .checked_add(*value)
                            .ok_or(ProjectionError::ArithmeticOverflow)?;
                    }
                }
                if demand > *capacity {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

fn compare(left: i64, op: ComparisonOp, right: i64) -> bool {
    match op {
        ComparisonOp::Equal => left == right,
        ComparisonOp::LessOrEqual => left <= right,
        ComparisonOp::GreaterOrEqual => left >= right,
    }
}

fn any_literals(
    literals: &[Literal],
    candidate: &CandidateValues,
) -> Result<bool, ProjectionError> {
    for literal in literals {
        if literal_value(literal, candidate)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn all_literals(
    literals: &[Literal],
    candidate: &CandidateValues,
) -> Result<bool, ProjectionError> {
    for literal in literals {
        if !literal_value(literal, candidate)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn true_count(literals: &[Literal], candidate: &CandidateValues) -> Result<u64, ProjectionError> {
    let mut count = 0_u64;
    for literal in literals {
        if literal_value(literal, candidate)? {
            count = count
                .checked_add(1)
                .ok_or(ProjectionError::ArithmeticOverflow)?;
        }
    }
    Ok(count)
}

fn literal_value(literal: &Literal, candidate: &CandidateValues) -> Result<bool, ProjectionError> {
    candidate
        .booleans
        .get(&literal.variable)
        .copied()
        .map(|value| value == literal.positive)
        .ok_or_else(|| ProjectionError::MissingRequiredValue(literal.variable.to_string()))
}

fn integer(id: &crate::IntVariableId, candidate: &CandidateValues) -> Result<i64, ProjectionError> {
    candidate
        .integers
        .get(id)
        .copied()
        .ok_or_else(|| ProjectionError::MissingRequiredValue(id.to_string()))
}

fn linear_value(
    expression: &crate::LinearExpression,
    candidate: &CandidateValues,
) -> Result<i64, ProjectionError> {
    let mut value = expression.constant;
    for term in &expression.terms {
        let product = term
            .coefficient
            .checked_mul(integer(&term.variable, candidate)?)
            .ok_or(ProjectionError::ArithmeticOverflow)?;
        value = value
            .checked_add(product)
            .ok_or(ProjectionError::ArithmeticOverflow)?;
    }
    Ok(value)
}

fn row_values(
    variables: &[crate::IntVariableId],
    candidate: &CandidateValues,
) -> Result<Vec<i64>, ProjectionError> {
    variables.iter().map(|id| integer(id, candidate)).collect()
}

fn intervals_by_id(
    problem: &PlanningProblem,
) -> BTreeMap<&crate::IntervalVariableId, &IntervalVariable> {
    problem
        .variables
        .iter()
        .filter_map(|variable| match variable {
            Variable::Interval(interval) => Some((&interval.id, interval)),
            Variable::Boolean(_) | Variable::Integer(_) => None,
        })
        .collect()
}

fn present_interval(
    interval: Option<&IntervalVariable>,
    candidate: &CandidateValues,
) -> Result<Option<(i64, i64)>, ProjectionError> {
    let interval =
        interval.ok_or_else(|| ProjectionError::UnknownCandidateValue("interval".to_owned()))?;
    if let Some(presence) = &interval.presence
        && !literal_value(presence, candidate)?
    {
        return Ok(None);
    }
    let start = integer(&interval.start, candidate)?;
    let duration = integer(&interval.duration, candidate)?;
    let end = integer(&interval.end, candidate)?;
    if duration < 0 || start.checked_add(duration) != Some(end) {
        return Err(ProjectionError::InvalidInterval(interval.id.clone()));
    }
    Ok(Some((start, end)))
}
