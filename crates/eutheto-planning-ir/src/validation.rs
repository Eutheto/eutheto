//! Strict bounded validation for untrusted schema-v1 planning IR.

use crate::analysis::{analyze_components, constraint_nodes, feature_usage, projection_nodes};
use crate::ids::{BoolVariableId, IntVariableId, IntervalVariableId, ProvenanceId};
use crate::model::{
    Constraint, IntDomain, LinearComparison, LinearExpression, Literal, PLANNING_IR_SCHEMA_VERSION,
    PlanningMetadata, PlanningProblem, ProjectionExpression, ProvenanceParameter, Variable,
};
use eutheto_domain_ir::AssignmentValue;
use eutheto_types::ResourceLimits;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Independent planning-IR safety limits. Scenario document limits remain separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanningIrLimitsV1 {
    pub max_ir_bytes: u64,
    pub max_variables: u64,
    pub max_constraints: u64,
    pub max_assumptions: u64,
    pub max_objective_levels: u64,
    pub max_objective_terms: u64,
    pub max_provenance_records: u64,
    pub max_provenance_depth: u64,
    pub max_parameters_per_record: u64,
    pub max_parameter_text_bytes: u64,
    pub max_entity_refs_per_record: u64,
    pub max_projections: u64,
    pub max_projection_expression_depth: u64,
    pub max_domain_ranges: u64,
    pub max_refs_per_node: u64,
    pub max_total_refs: u64,
    pub max_table_rows: u64,
    pub max_table_arity: u64,
    pub max_table_cells: u64,
    pub max_intervals_per_global: u64,
    pub max_enforcement_literals: u64,
    pub max_tags: u64,
    pub max_component_nodes: u64,
    pub max_component_edges: u64,
    pub max_id_bytes: u64,
    pub max_metadata_text_bytes: u64,
    pub max_abs_coefficient: i64,
    pub max_abs_value: i64,
}

impl PlanningIrLimitsV1 {
    /// Fixed schema-v1 limits.
    pub const DEFAULT: Self = Self {
        max_ir_bytes: 64 * 1024 * 1024,
        max_variables: 100_000,
        max_constraints: 500_000,
        max_assumptions: 100_000,
        max_objective_levels: 16,
        max_objective_terms: 100_000,
        max_provenance_records: 500_000,
        max_provenance_depth: 32,
        max_parameters_per_record: 64,
        max_parameter_text_bytes: 1024,
        max_entity_refs_per_record: 1024,
        max_projections: 100_000,
        max_projection_expression_depth: 16,
        max_domain_ranges: 64,
        max_refs_per_node: 100_000,
        max_total_refs: 5_000_000,
        max_table_rows: 100_000,
        max_table_arity: 64,
        max_table_cells: 1_000_000,
        max_intervals_per_global: 100_000,
        max_enforcement_literals: 1024,
        max_tags: 32,
        max_component_nodes: 100_000,
        max_component_edges: 5_000_000,
        max_id_bytes: 160,
        max_metadata_text_bytes: 4 * 1024,
        max_abs_coefficient: 1_000_000_000_000,
        max_abs_value: 1_000_000_000_000_000,
    };

    /// Applies caller resource caps only where the shared contract permits tightening.
    #[must_use]
    pub const fn tightened_by(mut self, caller: ResourceLimits) -> Self {
        self.max_variables = min_u64(self.max_variables, caller.max_variables);
        self.max_constraints = min_u64(self.max_constraints, caller.max_constraints);
        self
    }
}

const fn min_u64(left: u64, right: u64) -> u64 {
    if left < right { left } else { right }
}

/// Stable validation code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationCode {
    UnsupportedVersion,
    MalformedJson,
    LimitExceeded,
    DuplicateId,
    MissingReference,
    MissingProvenance,
    NonCanonical,
    InvalidDomain,
    InvalidShape,
    ArithmeticOverflow,
    ValueOutOfBounds,
    InvalidObjectiveBounds,
    InvalidProjection,
    ProvenanceCycle,
    UndeclaredCapability,
    InvalidSplitAuthorization,
}

/// Bounded, non-secret validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError {
    pub code: ValidationCode,
    pub path: String,
}

impl ValidationError {
    fn new(code: ValidationCode, path: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "planning IR {:?} at {}", self.code, self.path)
    }
}

impl std::error::Error for ValidationError {}

/// Parses unknown input only after enforcing the byte ceiling, then validates every invariant.
///
/// # Errors
/// Rejects oversized/malformed JSON, unknown fields/versions, and any validation failure.
pub fn parse_and_validate(
    bytes: &[u8],
    limits: PlanningIrLimitsV1,
) -> Result<PlanningProblem, ValidationError> {
    if count(bytes.len())? > limits.max_ir_bytes {
        return Err(ValidationError::new(ValidationCode::LimitExceeded, "bytes"));
    }
    let problem: PlanningProblem = serde_json::from_slice(bytes)
        .map_err(|_| ValidationError::new(ValidationCode::MalformedJson, "json"))?;
    validate(&problem, limits)?;
    Ok(problem)
}

/// Strictly validates canonical schema-v1 IR.
///
/// # Errors
/// Returns the first deterministic validation failure.
pub fn validate(
    problem: &PlanningProblem,
    limits: PlanningIrLimitsV1,
) -> Result<(), ValidationError> {
    validate_root(problem, limits)?;
    let (references, mut total_refs) = validate_variables(problem, limits)?;
    validate_constraints(problem, &references, limits, &mut total_refs)?;
    validate_objectives(
        problem,
        &references.ints,
        &references.provenance,
        limits,
        &mut total_refs,
    )?;
    validate_assumptions(
        problem,
        &references.bools,
        &references.provenance,
        &mut total_refs,
        limits,
    )?;
    validate_projections(
        problem,
        &references.bools,
        &references.ints,
        &references.intervals,
        &references.provenance,
        &mut total_refs,
        limits,
    )?;
    validate_provenance(problem, limits)?;
    validate_metadata(&problem.metadata, limits)?;
    validate_features_and_components(problem, total_refs, limits)
}

fn validate_root(
    problem: &PlanningProblem,
    limits: PlanningIrLimitsV1,
) -> Result<(), ValidationError> {
    if problem.schema_version != PLANNING_IR_SCHEMA_VERSION {
        return Err(ValidationError::new(
            ValidationCode::UnsupportedVersion,
            "schemaVersion",
        ));
    }
    check_limit(problem.variables.len(), limits.max_variables, "variables")?;
    check_limit(
        problem.constraints.len(),
        limits.max_constraints,
        "constraints",
    )?;
    check_limit(
        problem.assumptions.len(),
        limits.max_assumptions,
        "assumptions",
    )?;
    check_limit(
        problem.objectives.levels.len(),
        limits.max_objective_levels,
        "objectives.levels",
    )?;
    check_limit(
        problem.projections.len(),
        limits.max_projections,
        "projections",
    )?;
    check_limit(
        problem.provenance.len(),
        limits.max_provenance_records,
        "provenance",
    )?;
    validate_root_order(problem)
}

struct VariableReferences<'a> {
    provenance: BTreeSet<&'a ProvenanceId>,
    bools: BTreeSet<&'a BoolVariableId>,
    ints: BTreeMap<&'a IntVariableId, &'a IntDomain>,
    intervals: BTreeSet<&'a IntervalVariableId>,
}

fn validate_variables(
    problem: &PlanningProblem,
    limits: PlanningIrLimitsV1,
) -> Result<(VariableReferences<'_>, u64), ValidationError> {
    let provenance = problem.provenance.iter().map(|record| &record.id).collect();
    let mut bools = BTreeSet::new();
    let mut ints = BTreeMap::new();
    let mut intervals = BTreeSet::new();
    let mut all_ids = BTreeSet::new();
    let mut total_refs = 0_u64;
    for (index, variable) in problem.variables.iter().enumerate() {
        let (id, provenance_id) = match variable {
            Variable::Boolean(value) => {
                if !bools.insert(&value.id) {
                    return duplicate(format!("variables[{index}].id"));
                }
                (value.id.as_str(), &value.provenance)
            }
            Variable::Integer(value) => {
                validate_domain(&value.domain, limits, &format!("variables[{index}].domain"))?;
                if ints.insert(&value.id, &value.domain).is_some() {
                    return duplicate(format!("variables[{index}].id"));
                }
                (value.id.as_str(), &value.provenance)
            }
            Variable::Interval(value) => {
                if !intervals.insert(&value.id) {
                    return duplicate(format!("variables[{index}].id"));
                }
                total_refs = add_refs(total_refs, 4, limits)?;
                (value.id.as_str(), &value.provenance)
            }
        };
        if !all_ids.insert(id) {
            return duplicate(format!("variables[{index}].id"));
        }
        require_provenance(provenance_id, &provenance, &format!("variables[{index}]"))?;
    }
    let references = VariableReferences {
        provenance,
        bools,
        ints,
        intervals,
    };
    validate_interval_variables(problem, &references)?;
    Ok((references, total_refs))
}

fn validate_interval_variables(
    problem: &PlanningProblem,
    references: &VariableReferences<'_>,
) -> Result<(), ValidationError> {
    for (index, variable) in problem.variables.iter().enumerate() {
        if let Variable::Interval(interval) = variable {
            let start = require_int(
                &interval.start,
                &references.ints,
                &format!("variables[{index}].start"),
            )?;
            let duration = require_int(
                &interval.duration,
                &references.ints,
                &format!("variables[{index}].duration"),
            )?;
            let end = require_int(
                &interval.end,
                &references.ints,
                &format!("variables[{index}].end"),
            )?;
            if let Some(literal) = &interval.presence {
                require_literal(
                    literal,
                    &references.bools,
                    &format!("variables[{index}].presence"),
                )?;
            } else if duration
                .inclusive_ranges
                .iter()
                .any(|range| range.start < 0)
            {
                return Err(ValidationError::new(
                    ValidationCode::InvalidDomain,
                    format!("variables[{index}].duration"),
                ));
            }
            validate_interval_equation(start, duration, end, index)?;
        }
    }
    Ok(())
}

fn validate_interval_equation(
    start: &IntDomain,
    duration: &IntDomain,
    end: &IntDomain,
    index: usize,
) -> Result<(), ValidationError> {
    let mut has_representable_sum = false;
    let has_nonnegative_duration = duration.inclusive_ranges.iter().any(|range| range.end >= 0);
    for start_range in &start.inclusive_ranges {
        for duration_range in &duration.inclusive_ranges {
            let duration_minimum = duration_range.start.max(0);
            if duration_minimum > duration_range.end {
                continue;
            }
            let Some(sum_minimum) = start_range.start.checked_add(duration_minimum) else {
                continue;
            };
            has_representable_sum = true;
            let sum_maximum = start_range
                .end
                .checked_add(duration_range.end)
                .unwrap_or(i64::MAX);
            if domain_intersects_range(end, sum_minimum, sum_maximum) {
                return Ok(());
            }
        }
    }
    let path = format!("variables[{index}]");
    Err(ValidationError::new(
        if !has_nonnegative_duration || has_representable_sum {
            ValidationCode::InvalidDomain
        } else {
            ValidationCode::ArithmeticOverflow
        },
        path,
    ))
}

fn domain_intersects_range(domain: &IntDomain, minimum: i64, maximum: i64) -> bool {
    let first_after_maximum = domain
        .inclusive_ranges
        .partition_point(|range| range.start <= maximum);
    first_after_maximum != 0
        && domain.inclusive_ranges[..first_after_maximum]
            .last()
            .is_some_and(|range| range.end >= minimum)
}

fn validate_constraints(
    problem: &PlanningProblem,
    references: &VariableReferences<'_>,
    limits: PlanningIrLimitsV1,
    total_refs: &mut u64,
) -> Result<(), ValidationError> {
    let mut constraint_ids = BTreeSet::new();
    for (index, record) in problem.constraints.iter().enumerate() {
        if !constraint_ids.insert(&record.id) {
            return duplicate(format!("constraints[{index}].id"));
        }
        require_provenance(
            &record.provenance,
            &references.provenance,
            &format!("constraints[{index}]"),
        )?;
        check_limit(
            record.enforcement.len(),
            limits.max_enforcement_literals,
            &format!("constraints[{index}].enforcement"),
        )?;
        check_limit(
            record.tags.len(),
            limits.max_tags,
            &format!("constraints[{index}].tags"),
        )?;
        strict(
            &record.enforcement,
            &format!("constraints[{index}].enforcement"),
        )?;
        strict(&record.tags, &format!("constraints[{index}].tags"))?;
        for literal in &record.enforcement {
            require_literal(
                literal,
                &references.bools,
                &format!("constraints[{index}].enforcement"),
            )?;
        }
        let refs = constraint_nodes(&record.body).len();
        check_limit(
            refs,
            limits.max_refs_per_node,
            &format!("constraints[{index}].body"),
        )?;
        *total_refs = add_refs(*total_refs, count(refs)?, limits)?;
        validate_constraint(
            &record.body,
            &references.bools,
            &references.ints,
            &references.intervals,
            limits,
            &format!("constraints[{index}].body"),
        )?;
    }
    Ok(())
}

fn validate_features_and_components(
    problem: &PlanningProblem,
    total_refs: u64,
    limits: PlanningIrLimitsV1,
) -> Result<(), ValidationError> {
    let usage = feature_usage(problem);
    if !usage
        .required_capabilities()
        .is_subset(&problem.declared_capabilities)
    {
        return Err(ValidationError::new(
            ValidationCode::UndeclaredCapability,
            "declaredCapabilities",
        ));
    }
    if usage.table_row_count > limits.max_table_rows
        || usage.table_cell_count > limits.max_table_cells
    {
        return Err(ValidationError::new(
            ValidationCode::LimitExceeded,
            "tables",
        ));
    }
    if total_refs > limits.max_total_refs {
        return Err(ValidationError::new(
            ValidationCode::LimitExceeded,
            "totalReferences",
        ));
    }
    let components = analyze_components(problem);
    check_limit(
        components.components.len(),
        limits.max_component_nodes,
        "components.nodes",
    )?;
    if components.edge_count > limits.max_component_edges {
        return Err(ValidationError::new(
            ValidationCode::LimitExceeded,
            "components.edges",
        ));
    }
    if let Some(authorization) = &problem.split_authorization
        && (authorization.component_hash != components.component_hash
            || authorization.domain_merge_contract.is_empty()
            || count(authorization.domain_merge_contract.len())? > limits.max_metadata_text_bytes
            || !authorization.projection_independent)
    {
        return Err(ValidationError::new(
            ValidationCode::InvalidSplitAuthorization,
            "splitAuthorization",
        ));
    }
    Ok(())
}

fn validate_root_order(problem: &PlanningProblem) -> Result<(), ValidationError> {
    if !problem
        .variables
        .windows(2)
        .all(|pair| pair[0].canonical_id() < pair[1].canonical_id())
    {
        return noncanonical("variables");
    }
    strict_by(&problem.constraints, |record| &record.id, "constraints")?;
    strict_by(&problem.assumptions, |record| &record.id, "assumptions")?;
    strict_by(&problem.projections, |record| &record.id, "projections")?;
    strict_by(&problem.provenance, |record| &record.id, "provenance")
}

fn validate_domain(
    domain: &IntDomain,
    limits: PlanningIrLimitsV1,
    path: &str,
) -> Result<(), ValidationError> {
    check_limit(
        domain.inclusive_ranges.len(),
        limits.max_domain_ranges,
        path,
    )?;
    let normalized = IntDomain::new(domain.inclusive_ranges.clone())
        .map_err(|_| ValidationError::new(ValidationCode::InvalidDomain, path))?;
    if normalized != *domain {
        return noncanonical(path);
    }
    for range in &domain.inclusive_ranges {
        check_value(range.start, limits, path)?;
        check_value(range.end, limits, path)?;
    }
    Ok(())
}

fn validate_constraint(
    constraint: &Constraint,
    bools: &BTreeSet<&BoolVariableId>,
    ints: &BTreeMap<&IntVariableId, &IntDomain>,
    intervals: &BTreeSet<&IntervalVariableId>,
    limits: PlanningIrLimitsV1,
    path: &str,
) -> Result<(), ValidationError> {
    match constraint {
        Constraint::BoolOr { literals }
        | Constraint::BoolAnd { literals }
        | Constraint::AtMostOne { literals }
        | Constraint::ExactlyOne { literals } => {
            validate_literals(literals, bools, path)?;
        }
        Constraint::CardinalityRange { literals, min, max } => {
            validate_literals(literals, bools, path)?;
            let len = count(literals.len())?;
            if min > max || *max > len {
                return shape(path);
            }
        }
        Constraint::Implication {
            antecedent,
            consequent,
        } => {
            require_literal(antecedent, bools, path)?;
            require_literal(consequent, bools, path)?;
        }
        Constraint::Equivalence { left, right } => {
            require_literal(left, bools, path)?;
            require_literal(right, bools, path)?;
        }
        Constraint::LinearComparison(comparison) => {
            validate_comparison(comparison, ints, limits, path)?;
        }
        Constraint::ReifiedLinearComparison {
            literal,
            comparison,
        } => {
            require_literal(literal, bools, path)?;
            validate_comparison(comparison, ints, limits, path)?;
        }
        Constraint::AllDifferent { variables } => {
            strict(variables, path)?;
            for variable in variables {
                require_int(variable, ints, path)?;
            }
        }
        Constraint::AllowedTable { variables, rows }
        | Constraint::ForbiddenTable { variables, rows } => {
            validate_table(variables, rows, ints, limits, path)?;
        }
        Constraint::Element {
            index,
            values,
            target,
        } => {
            require_int(index, ints, path)?;
            require_int(target, ints, path)?;
            for value in values {
                check_value(*value, limits, path)?;
            }
        }
        Constraint::Min { target, inputs } | Constraint::Max { target, inputs } => {
            validate_integer_aggregate(target, inputs, ints, path)?;
        }
        Constraint::Equality { left, right } => {
            require_int(left, ints, path)?;
            require_int(right, ints, path)?;
        }
        Constraint::AbsDifference {
            target,
            left,
            right,
        } => {
            require_int(target, ints, path)?;
            let left_domain = require_int(left, ints, path)?;
            let right_domain = require_int(right, ints, path)?;
            prove_abs_bounds(left_domain, right_domain, path)?;
        }
        Constraint::NoOverlap { intervals: values } => {
            strict(values, path)?;
            check_limit(values.len(), limits.max_intervals_per_global, path)?;
            for interval in values {
                require_interval(interval, intervals, path)?;
            }
        }
        Constraint::Cumulative {
            intervals: values,
            demands,
            capacity,
        } => {
            validate_cumulative(values, demands, *capacity, intervals, limits, path)?;
        }
    }
    Ok(())
}

fn validate_integer_aggregate(
    target: &IntVariableId,
    inputs: &[IntVariableId],
    ints: &BTreeMap<&IntVariableId, &IntDomain>,
    path: &str,
) -> Result<(), ValidationError> {
    require_int(target, ints, path)?;
    if inputs.is_empty() {
        return shape(path);
    }
    strict(inputs, path)?;
    for input in inputs {
        require_int(input, ints, path)?;
    }
    Ok(())
}

fn validate_table(
    variables: &[IntVariableId],
    rows: &[Vec<i64>],
    ints: &BTreeMap<&IntVariableId, &IntDomain>,
    limits: PlanningIrLimitsV1,
    path: &str,
) -> Result<(), ValidationError> {
    check_limit(variables.len(), limits.max_table_arity, path)?;
    check_limit(rows.len(), limits.max_table_rows, path)?;
    strict(rows, path)?;
    for variable in variables {
        require_int(variable, ints, path)?;
    }
    let arity = variables.len();
    if rows.iter().any(|row| row.len() != arity) {
        return shape(path);
    }
    let cells = count(rows.len())?
        .checked_mul(count(arity)?)
        .ok_or_else(|| ValidationError::new(ValidationCode::ArithmeticOverflow, path))?;
    if cells > limits.max_table_cells {
        return Err(ValidationError::new(ValidationCode::LimitExceeded, path));
    }
    for value in rows.iter().flatten() {
        check_value(*value, limits, path)?;
    }
    Ok(())
}

fn validate_cumulative(
    values: &[IntervalVariableId],
    demands: &[i64],
    capacity: i64,
    intervals: &BTreeSet<&IntervalVariableId>,
    limits: PlanningIrLimitsV1,
    path: &str,
) -> Result<(), ValidationError> {
    if values.len() != demands.len() || capacity < 0 {
        return shape(path);
    }
    strict(values, path)?;
    check_limit(values.len(), limits.max_intervals_per_global, path)?;
    check_value(capacity, limits, path)?;
    let mut total = 0_i64;
    for (interval, demand) in values.iter().zip(demands) {
        require_interval(interval, intervals, path)?;
        if *demand < 0 {
            return shape(path);
        }
        check_value(*demand, limits, path)?;
        total = total
            .checked_add(*demand)
            .ok_or_else(|| ValidationError::new(ValidationCode::ArithmeticOverflow, path))?;
    }
    Ok(())
}

fn validate_objectives(
    problem: &PlanningProblem,
    ints: &BTreeMap<&IntVariableId, &IntDomain>,
    provenance: &BTreeSet<&ProvenanceId>,
    limits: PlanningIrLimitsV1,
    total_refs: &mut u64,
) -> Result<(), ValidationError> {
    let mut level_ids = BTreeSet::new();
    let mut term_count = 0_u64;
    for (level_index, level) in problem.objectives.levels.iter().enumerate() {
        if !level_ids.insert(&level.id) {
            return duplicate(format!("objectives.levels[{level_index}].id"));
        }
        require_provenance(
            &level.provenance,
            provenance,
            &format!("objectives.levels[{level_index}]"),
        )?;
        if level.lower_bound > level.upper_bound {
            return Err(ValidationError::new(
                ValidationCode::InvalidObjectiveBounds,
                format!("objectives.levels[{level_index}]"),
            ));
        }
        check_value(level.lower_bound, limits, "objectives.lowerBound")?;
        check_value(level.upper_bound, limits, "objectives.upperBound")?;
        strict_by(&level.terms, |term| &term.id, "objectives.terms")?;
        let mut exact_min = 0_i64;
        let mut exact_max = 0_i64;
        for term in &level.terms {
            term_count = term_count.checked_add(1).ok_or_else(|| {
                ValidationError::new(ValidationCode::ArithmeticOverflow, "objectives.terms")
            })?;
            require_provenance(&term.provenance, provenance, "objectives.term")?;
            let (min, max) = expression_bounds(&term.expression, ints, limits, "objectives.term")?;
            if min < 0 {
                return Err(ValidationError::new(
                    ValidationCode::InvalidObjectiveBounds,
                    format!("objectives.levels[{level_index}].terms"),
                ));
            }
            exact_min = exact_min.checked_add(min).ok_or_else(|| {
                ValidationError::new(ValidationCode::ArithmeticOverflow, "objectives.lowerBound")
            })?;
            exact_max = exact_max.checked_add(max).ok_or_else(|| {
                ValidationError::new(ValidationCode::ArithmeticOverflow, "objectives.upperBound")
            })?;
            *total_refs = add_refs(*total_refs, count(term.expression.terms.len())?, limits)?;
        }
        if exact_min != level.lower_bound || exact_max != level.upper_bound {
            return Err(ValidationError::new(
                ValidationCode::InvalidObjectiveBounds,
                format!("objectives.levels[{level_index}]"),
            ));
        }
    }
    if term_count > limits.max_objective_terms {
        return Err(ValidationError::new(
            ValidationCode::LimitExceeded,
            "objectives.terms",
        ));
    }
    Ok(())
}

fn validate_assumptions(
    problem: &PlanningProblem,
    bools: &BTreeSet<&BoolVariableId>,
    provenance: &BTreeSet<&ProvenanceId>,
    total_refs: &mut u64,
    limits: PlanningIrLimitsV1,
) -> Result<(), ValidationError> {
    for assumption in &problem.assumptions {
        require_literal(&assumption.literal, bools, "assumptions.literal")?;
        require_provenance(&assumption.provenance, provenance, "assumptions")?;
        *total_refs = add_refs(*total_refs, 1, limits)?;
    }
    Ok(())
}

fn validate_projections(
    problem: &PlanningProblem,
    bools: &BTreeSet<&BoolVariableId>,
    ints: &BTreeMap<&IntVariableId, &IntDomain>,
    intervals: &BTreeSet<&IntervalVariableId>,
    provenance: &BTreeSet<&ProvenanceId>,
    total_refs: &mut u64,
    limits: PlanningIrLimitsV1,
) -> Result<(), ValidationError> {
    let mut assignments = BTreeSet::new();
    for projection in &problem.projections {
        if !assignments.insert(&projection.assignment_id) {
            return duplicate("projections.assignmentId");
        }
        require_provenance(&projection.provenance, provenance, "projections")?;
        match &projection.expression {
            ProjectionExpression::Boolean(id) => {
                require_bool(id, bools, "projections.expression")?;
            }
            ProjectionExpression::Integer(id) => {
                require_int(id, ints, "projections.expression")?;
            }
            ProjectionExpression::Linear(expression) => {
                expression_bounds(expression, ints, limits, "projections.expression")?;
            }
            ProjectionExpression::Interval(id) => {
                require_interval(id, intervals, "projections.expression")?;
            }
            ProjectionExpression::Constant(AssignmentValue::Interval(interval)) => {
                interval.validate().map_err(|_| {
                    ValidationError::new(
                        ValidationCode::InvalidProjection,
                        "projections.expression",
                    )
                })?;
            }
            ProjectionExpression::Constant(_) => {}
        }
        let refs = projection_nodes(&projection.expression).len();
        check_limit(refs, limits.max_refs_per_node, "projections.expression")?;
        *total_refs = add_refs(*total_refs, count(refs)?, limits)?;
        if limits.max_projection_expression_depth < 1 {
            return Err(ValidationError::new(
                ValidationCode::LimitExceeded,
                "projections.expressionDepth",
            ));
        }
    }
    Ok(())
}

fn validate_provenance(
    problem: &PlanningProblem,
    limits: PlanningIrLimitsV1,
) -> Result<(), ValidationError> {
    let records: BTreeMap<_, _> = problem
        .provenance
        .iter()
        .map(|record| (&record.id, record))
        .collect();
    for record in &problem.provenance {
        check_limit(
            record.entity_refs.len(),
            limits.max_entity_refs_per_record,
            "provenance.entityRefs",
        )?;
        check_limit(
            record.parameters.len(),
            limits.max_parameters_per_record,
            "provenance.parameters",
        )?;
        strict(&record.entity_refs, "provenance.entityRefs")?;
        check_text(
            &record.source_id,
            limits.max_metadata_text_bytes,
            "provenance.sourceId",
        )?;
        check_text(
            &record.message_key,
            limits.max_metadata_text_bytes,
            "provenance.messageKey",
        )?;
        for (key, value) in &record.parameters {
            check_text(
                key,
                limits.max_parameter_text_bytes,
                "provenance.parameters.key",
            )?;
            if let ProvenanceParameter::Text(text) = value {
                check_text(
                    text,
                    limits.max_parameter_text_bytes,
                    "provenance.parameters.text",
                )?;
            }
        }
        let mut seen = BTreeSet::new();
        let mut current = Some(&record.id);
        let mut depth = 0_u64;
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(ValidationError::new(
                    ValidationCode::ProvenanceCycle,
                    "provenance.parent",
                ));
            }
            depth = depth.checked_add(1).ok_or_else(|| {
                ValidationError::new(ValidationCode::ArithmeticOverflow, "provenance.depth")
            })?;
            if depth > limits.max_provenance_depth {
                return Err(ValidationError::new(
                    ValidationCode::LimitExceeded,
                    "provenance.depth",
                ));
            }
            let Some(found) = records.get(id) else {
                return Err(ValidationError::new(
                    ValidationCode::MissingReference,
                    "provenance.parent",
                ));
            };
            current = found.parent.as_ref();
        }
    }
    Ok(())
}

fn validate_metadata(
    metadata: &PlanningMetadata,
    limits: PlanningIrLimitsV1,
) -> Result<(), ValidationError> {
    check_text(
        &metadata.compiler_version,
        limits.max_metadata_text_bytes,
        "metadata.compilerVersion",
    )?;
    for (key, value) in &metadata.compile_metadata {
        check_text(
            key.as_str(),
            limits.max_id_bytes,
            "metadata.compileMetadata.key",
        )?;
        if let ProvenanceParameter::Text(text) = value {
            check_text(
                text,
                limits.max_metadata_text_bytes,
                "metadata.compileMetadata.text",
            )?;
        }
    }
    for (key, value) in &metadata.display_text {
        check_text(
            key,
            limits.max_metadata_text_bytes,
            "metadata.displayText.key",
        )?;
        check_text(
            value,
            limits.max_metadata_text_bytes,
            "metadata.displayText.value",
        )?;
    }
    Ok(())
}

fn validate_comparison(
    comparison: &LinearComparison,
    ints: &BTreeMap<&IntVariableId, &IntDomain>,
    limits: PlanningIrLimitsV1,
    path: &str,
) -> Result<(), ValidationError> {
    expression_bounds(&comparison.expression, ints, limits, path)?;
    check_value(comparison.rhs, limits, path)
}

pub(crate) fn expression_bounds(
    expression: &LinearExpression,
    ints: &BTreeMap<&IntVariableId, &IntDomain>,
    limits: PlanningIrLimitsV1,
    path: &str,
) -> Result<(i64, i64), ValidationError> {
    strict_by(&expression.terms, |term| &term.variable, path)?;
    check_value(expression.constant, limits, path)?;
    let mut minimum = expression.constant;
    let mut maximum = expression.constant;
    for term in &expression.terms {
        if term.coefficient == 0
            || term.coefficient.unsigned_abs() > limits.max_abs_coefficient.cast_unsigned()
        {
            return Err(ValidationError::new(ValidationCode::ValueOutOfBounds, path));
        }
        let domain = require_int(&term.variable, ints, path)?;
        let (lower, upper) = domain
            .bounds()
            .ok_or_else(|| ValidationError::new(ValidationCode::InvalidDomain, path))?;
        let first = term
            .coefficient
            .checked_mul(lower)
            .ok_or_else(|| ValidationError::new(ValidationCode::ArithmeticOverflow, path))?;
        let second = term
            .coefficient
            .checked_mul(upper)
            .ok_or_else(|| ValidationError::new(ValidationCode::ArithmeticOverflow, path))?;
        minimum = minimum
            .checked_add(first.min(second))
            .ok_or_else(|| ValidationError::new(ValidationCode::ArithmeticOverflow, path))?;
        maximum = maximum
            .checked_add(first.max(second))
            .ok_or_else(|| ValidationError::new(ValidationCode::ArithmeticOverflow, path))?;
    }
    Ok((minimum, maximum))
}

fn prove_abs_bounds(
    left: &IntDomain,
    right: &IntDomain,
    path: &str,
) -> Result<(), ValidationError> {
    let (left_min, left_max) = left
        .bounds()
        .ok_or_else(|| ValidationError::new(ValidationCode::InvalidDomain, path))?;
    let (right_min, right_max) = right
        .bounds()
        .ok_or_else(|| ValidationError::new(ValidationCode::InvalidDomain, path))?;
    left_min
        .checked_sub(right_max)
        .and_then(i64::checked_abs)
        .ok_or_else(|| ValidationError::new(ValidationCode::ArithmeticOverflow, path))?;
    left_max
        .checked_sub(right_min)
        .and_then(i64::checked_abs)
        .ok_or_else(|| ValidationError::new(ValidationCode::ArithmeticOverflow, path))?;
    Ok(())
}

fn validate_literals(
    literals: &[Literal],
    bools: &BTreeSet<&BoolVariableId>,
    path: &str,
) -> Result<(), ValidationError> {
    strict(literals, path)?;
    for literal in literals {
        require_literal(literal, bools, path)?;
    }
    Ok(())
}

fn require_literal(
    literal: &Literal,
    bools: &BTreeSet<&BoolVariableId>,
    path: &str,
) -> Result<(), ValidationError> {
    require_bool(&literal.variable, bools, path)
}

fn require_bool(
    id: &BoolVariableId,
    bools: &BTreeSet<&BoolVariableId>,
    path: &str,
) -> Result<(), ValidationError> {
    if bools.contains(id) {
        Ok(())
    } else {
        Err(ValidationError::new(ValidationCode::MissingReference, path))
    }
}

fn require_int<'a>(
    id: &IntVariableId,
    ints: &BTreeMap<&'a IntVariableId, &'a IntDomain>,
    path: &str,
) -> Result<&'a IntDomain, ValidationError> {
    ints.get(id)
        .copied()
        .ok_or_else(|| ValidationError::new(ValidationCode::MissingReference, path))
}

fn require_interval(
    id: &IntervalVariableId,
    intervals: &BTreeSet<&IntervalVariableId>,
    path: &str,
) -> Result<(), ValidationError> {
    if intervals.contains(id) {
        Ok(())
    } else {
        Err(ValidationError::new(ValidationCode::MissingReference, path))
    }
}

fn require_provenance(
    id: &ProvenanceId,
    provenance: &BTreeSet<&ProvenanceId>,
    path: &str,
) -> Result<(), ValidationError> {
    if provenance.contains(id) {
        Ok(())
    } else {
        Err(ValidationError::new(
            ValidationCode::MissingProvenance,
            path,
        ))
    }
}

fn strict<T: Ord>(values: &[T], path: &str) -> Result<(), ValidationError> {
    if values.windows(2).all(|pair| pair[0] < pair[1]) {
        Ok(())
    } else {
        noncanonical(path)
    }
}

fn strict_by<T, K: Ord>(
    values: &[T],
    key: impl Fn(&T) -> &K,
    path: &str,
) -> Result<(), ValidationError> {
    if values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1])) {
        Ok(())
    } else {
        noncanonical(path)
    }
}

fn check_value(value: i64, limits: PlanningIrLimitsV1, path: &str) -> Result<(), ValidationError> {
    if value.unsigned_abs() > limits.max_abs_value.cast_unsigned() {
        Err(ValidationError::new(ValidationCode::ValueOutOfBounds, path))
    } else {
        Ok(())
    }
}

fn check_text(value: &str, maximum: u64, path: &str) -> Result<(), ValidationError> {
    if value.is_empty() || count(value.len())? > maximum {
        Err(ValidationError::new(ValidationCode::LimitExceeded, path))
    } else {
        Ok(())
    }
}

fn check_limit(value: usize, maximum: u64, path: &str) -> Result<(), ValidationError> {
    if count(value)? > maximum {
        Err(ValidationError::new(ValidationCode::LimitExceeded, path))
    } else {
        Ok(())
    }
}

fn add_refs(
    total: u64,
    additional: u64,
    limits: PlanningIrLimitsV1,
) -> Result<u64, ValidationError> {
    let value = total.checked_add(additional).ok_or_else(|| {
        ValidationError::new(ValidationCode::ArithmeticOverflow, "totalReferences")
    })?;
    if value > limits.max_total_refs {
        Err(ValidationError::new(
            ValidationCode::LimitExceeded,
            "totalReferences",
        ))
    } else {
        Ok(value)
    }
}

fn count(value: usize) -> Result<u64, ValidationError> {
    u64::try_from(value)
        .map_err(|_| ValidationError::new(ValidationCode::ArithmeticOverflow, "count"))
}

fn duplicate<T>(path: impl Into<String>) -> Result<T, ValidationError> {
    Err(ValidationError::new(ValidationCode::DuplicateId, path))
}

fn noncanonical<T>(path: impl Into<String>) -> Result<T, ValidationError> {
    Err(ValidationError::new(ValidationCode::NonCanonical, path))
}

fn shape<T>(path: impl Into<String>) -> Result<T, ValidationError> {
    Err(ValidationError::new(ValidationCode::InvalidShape, path))
}
