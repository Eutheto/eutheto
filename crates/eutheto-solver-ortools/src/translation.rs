use std::collections::BTreeMap;

use eutheto_domain_ir::OptimizationDirection;
use eutheto_planning_ir::{
    BoolVariableId, ComparisonOp, Constraint, IntDomain, IntVariableId, LexicographicStrategy,
    LinearComparison, Literal, PlanningIrLimitsV1, PlanningProblem, ValidationError, Variable,
    lexicographic_strategy, validate,
};
use prost::Message;
use thiserror::Error;

use crate::cp_sat::{
    BoolArgumentProto, ConstraintProto, CpModelProto, CpObjectiveProto, IntegerVariableProto,
    LinearConstraintProto, constraint_proto,
};
const BOOLEAN_DOMAIN: [i64; 2] = [0, 1];

/// Deterministic CP-SAT scalar-variable declarations and their retained planning-ID maps.
///
/// This is a variable-only translation stage. It deliberately does not expose serialized model
/// bytes until the remaining constraints, objectives, assumptions, and projections have been
/// translated into the same model.
#[derive(Clone, Debug)]
pub struct VariableTranslation {
    model: CpModelProto,
    boolean_indices: BTreeMap<BoolVariableId, i32>,
    integer_indices: BTreeMap<IntVariableId, i32>,
}

impl VariableTranslation {
    /// Returns the number of scalar CP-SAT variables in canonical planning-variable order.
    #[must_use]
    pub fn variable_count(&self) -> usize {
        self.model.variables.len()
    }

    /// Returns the CP-SAT index retained for a Boolean planning variable.
    #[must_use]
    pub fn boolean_index(&self, id: &BoolVariableId) -> Option<i32> {
        self.boolean_indices.get(id).copied()
    }

    /// Returns the CP-SAT index retained for an integer planning variable.
    #[must_use]
    pub fn integer_index(&self, id: &IntVariableId) -> Option<i32> {
        self.integer_indices.get(id).copied()
    }
}
/// A complete serialized CP-SAT model for the planning features accepted by this translator.
///
/// The retained maps are the only link from planning identities to native indices. The native
/// protobuf contains no planning IDs, provenance, or display text.
#[derive(Clone, Debug)]
pub struct TranslatedCpSatModel {
    cp_model_proto: Vec<u8>,
    boolean_indices: BTreeMap<BoolVariableId, i32>,
    integer_indices: BTreeMap<IntVariableId, i32>,
}

impl TranslatedCpSatModel {
    /// Returns the encoded `operations_research.sat.CpModelProto`.
    #[must_use]
    pub fn cp_model_proto(&self) -> &[u8] {
        &self.cp_model_proto
    }

    /// Returns the CP-SAT index retained for a Boolean planning variable.
    #[must_use]
    pub fn boolean_index(&self, id: &BoolVariableId) -> Option<i32> {
        self.boolean_indices.get(id).copied()
    }

    /// Returns the CP-SAT index retained for an integer planning variable.
    #[must_use]
    pub fn integer_index(&self, id: &IntVariableId) -> Option<i32> {
        self.integer_indices.get(id).copied()
    }
}

/// Safe failures before a planning model can enter a worker request.
#[derive(Debug, Error)]
pub enum TranslationError {
    /// The solver-neutral model did not satisfy its complete bounded schema contract.
    #[error("planning IR validation failed before CP-SAT translation")]
    InvalidPlanningIr(#[from] ValidationError),
    /// CP-SAT variable references are signed 32-bit indices.
    #[error("the CP-SAT scalar variable index exceeds the signed 32-bit protocol range")]
    VariableIndexOverflow,
    /// Validated planning IR referenced a Boolean absent from the retained index map.
    #[error("validated planning IR referenced an unindexed Boolean variable")]
    MissingBooleanIndex,
    /// This incremental translator does not yet encode interval-variable semantics.
    #[error("interval variables are not supported by the current CP-SAT translator")]
    UnsupportedIntervalVariable,
    /// Validated planning IR referenced an integer absent from the retained index map.
    #[error("validated planning IR referenced an unindexed integer variable")]
    MissingIntegerIndex,
    /// This incremental translator does not yet encode this constraint primitive.
    #[error("the planning constraint is not supported by the current CP-SAT translator")]
    UnsupportedConstraint,
    /// Cardinality bounds and constant offsets must fit CP-SAT's signed integer domain.
    #[error("the cardinality constraint exceeds CP-SAT's signed 64-bit domain")]
    CardinalityBoundOverflow,
    /// OR-Tools rejects integer domains or conservative linear sums outside its safe range.
    #[error("the translated integer expression exceeds OR-Tools' native safe range")]
    NativeIntegerOverflow,
    /// This single-model adapter cannot execute exact lexicographic multipass solving.
    #[error("the objective requires unsupported multipass lexicographic solving")]
    UnsupportedMultipassObjective,
    /// Assumptions are translated in a later stage.
    #[error("assumptions are not supported by the current CP-SAT translator")]
    UnsupportedAssumption,
    /// Projections are translated in a later stage.
    #[error("projections are not supported by the current CP-SAT translator")]
    UnsupportedProjection,
}

/// Translates validated Boolean and integer domains into CP-SAT scalar declarations.
///
/// Boolean variables are integer variables with the exact inclusive domain `[0, 1]`. Integer
/// domain unions retain the planning IR's canonical inclusive-range order. Interval declarations
/// allocate no additional CP-SAT variable because their start, duration, and end components are
/// ordinary integer variables.
///
/// No planning IDs, provenance, or display text are copied into the native model.
///
/// # Errors
///
/// Returns the original planning validation failure, or an index-range failure if a caller uses a
/// future planning limit wider than CP-SAT's signed 32-bit variable-reference range.
pub fn translate_variable_domains(
    problem: &PlanningProblem,
    limits: PlanningIrLimitsV1,
) -> Result<VariableTranslation, TranslationError> {
    validate(problem, limits)?;
    translate_variable_domains_validated(problem, false)
}

fn translate_variable_domains_validated(
    problem: &PlanningProblem,
    include_constant_one: bool,
) -> Result<VariableTranslation, TranslationError> {
    let scalar_count = problem
        .variables
        .iter()
        .filter(|variable| !matches!(variable, Variable::Interval(_)))
        .count();
    let variable_capacity = scalar_count
        .checked_add(usize::from(include_constant_one))
        .ok_or(TranslationError::VariableIndexOverflow)?;
    let mut model = CpModelProto {
        variables: Vec::with_capacity(variable_capacity),
        ..CpModelProto::default()
    };
    if include_constant_one {
        model.variables.push(IntegerVariableProto {
            name: String::new(),
            domain: vec![1, 1],
        });
    }
    let mut boolean_indices = BTreeMap::new();
    let mut integer_indices = BTreeMap::new();

    for variable in &problem.variables {
        let index = i32::try_from(model.variables.len())
            .map_err(|_| TranslationError::VariableIndexOverflow)?;
        match variable {
            Variable::Boolean(boolean) => {
                model.variables.push(IntegerVariableProto {
                    name: String::new(),
                    domain: BOOLEAN_DOMAIN.to_vec(),
                });
                boolean_indices.insert(boolean.id.clone(), index);
            }
            Variable::Integer(integer) => {
                ensure_native_domain(&integer.domain)?;
                model.variables.push(IntegerVariableProto {
                    name: String::new(),
                    domain: flatten_domain(&integer.domain),
                });
                integer_indices.insert(integer.id.clone(), index);
            }
            Variable::Interval(_) => {}
        }
    }

    Ok(VariableTranslation {
        model,
        boolean_indices,
        integer_indices,
    })
}

/// Translates every currently supported planning primitive into a complete CP-SAT model.
///
/// At this stage the accepted model surface is Boolean and integer scalar variables plus Boolean
/// clauses, conjunctions, implications, equivalences, one-of constraints, cardinality ranges,
/// integer linear comparisons, and bounded objective terms. Exact lexicographic objectives are
/// scalarized with their proven planning-IR weights; maximize levels are sign-normalized into
/// CP-SAT's minimization objective. Unsupported primitives are rejected rather than omitted.
/// Positive literals use their scalar variable index; negative literals use CP-SAT's exact
/// `-index - 1` encoding. Common enforcement literals are copied onto every native constraint
/// generated for their planning record. Empty clauses and exactly-one constraints remain empty and
/// therefore false; empty conjunctions, at-most-one constraints, and the sole valid empty
/// cardinality range `0..=0` remain true.
/// # Errors
///
/// Returns a validation error for invalid planning IR and an explicit unsupported-feature error
/// whenever accepting the input would require silently dropping planning semantics.
pub fn translate_supported_model(
    problem: &PlanningProblem,
    limits: PlanningIrLimitsV1,
) -> Result<TranslatedCpSatModel, TranslationError> {
    validate(problem, limits)?;
    if problem
        .variables
        .iter()
        .any(|variable| matches!(variable, Variable::Interval(_)))
    {
        return Err(TranslationError::UnsupportedIntervalVariable);
    }
    let objective_weights = match lexicographic_strategy(&problem.objectives) {
        LexicographicStrategy::ExactScalarization { weights } => weights,
        LexicographicStrategy::Multipass => {
            return Err(TranslationError::UnsupportedMultipassObjective);
        }
    };
    if !problem.assumptions.is_empty() {
        return Err(TranslationError::UnsupportedAssumption);
    }
    if !problem.projections.is_empty() {
        return Err(TranslationError::UnsupportedProjection);
    }

    let needs_constant_one = problem.constraints.iter().any(|record| {
        matches!(
            &record.body,
            Constraint::LinearComparison(comparison) if comparison.expression.constant != 0
        )
    }) || problem
        .objectives
        .levels
        .iter()
        .any(|level| level.terms.iter().any(|term| term.expression.constant != 0));
    let VariableTranslation {
        mut model,
        boolean_indices,
        integer_indices,
    } = translate_variable_domains_validated(problem, needs_constant_one)?;
    translate_constraints(problem, &mut model, &boolean_indices, &integer_indices, 0)?;
    translate_objective(problem, &objective_weights, &mut model, &integer_indices, 0)?;

    Ok(TranslatedCpSatModel {
        cp_model_proto: model.encode_to_vec(),
        boolean_indices,
        integer_indices,
    })
}

fn translate_constraints(
    problem: &PlanningProblem,
    model: &mut CpModelProto,
    boolean_indices: &BTreeMap<BoolVariableId, i32>,
    integer_indices: &BTreeMap<IntVariableId, i32>,
    constant_one_index: i32,
) -> Result<(), TranslationError> {
    model.constraints.reserve(problem.constraints.len());
    for record in &problem.constraints {
        let enforcement = translate_literals(&record.enforcement, boolean_indices)?;
        let first_generated = model.constraints.len();
        match &record.body {
            Constraint::BoolOr { literals } => {
                model.constraints.push(boolean_constraint(
                    constraint_proto::Constraint::BoolOr,
                    translate_literals(literals, boolean_indices)?,
                ));
            }
            Constraint::BoolAnd { literals } => {
                model.constraints.push(boolean_constraint(
                    constraint_proto::Constraint::BoolAnd,
                    translate_literals(literals, boolean_indices)?,
                ));
            }
            Constraint::Implication {
                antecedent,
                consequent,
            } => {
                let antecedent = translate_literal(antecedent, boolean_indices)?;
                let consequent = translate_literal(consequent, boolean_indices)?;
                model.constraints.push(boolean_constraint(
                    constraint_proto::Constraint::BoolOr,
                    vec![!antecedent, consequent],
                ));
            }
            Constraint::Equivalence { left, right } => {
                let left = translate_literal(left, boolean_indices)?;
                let right = translate_literal(right, boolean_indices)?;
                model.constraints.push(boolean_constraint(
                    constraint_proto::Constraint::BoolOr,
                    vec![!left, right],
                ));
                model.constraints.push(boolean_constraint(
                    constraint_proto::Constraint::BoolOr,
                    vec![left, !right],
                ));
            }
            Constraint::AtMostOne { literals } => {
                if enforcement.is_empty() {
                    model.constraints.push(boolean_constraint(
                        constraint_proto::Constraint::AtMostOne,
                        translate_literals(literals, boolean_indices)?,
                    ));
                } else {
                    model.constraints.push(cardinality_constraint(
                        literals,
                        0,
                        1,
                        boolean_indices,
                    )?);
                }
            }
            Constraint::ExactlyOne { literals } => {
                if enforcement.is_empty() {
                    model.constraints.push(boolean_constraint(
                        constraint_proto::Constraint::ExactlyOne,
                        translate_literals(literals, boolean_indices)?,
                    ));
                } else {
                    model.constraints.push(cardinality_constraint(
                        literals,
                        1,
                        1,
                        boolean_indices,
                    )?);
                }
            }
            Constraint::CardinalityRange { literals, min, max } => {
                model.constraints.push(cardinality_constraint(
                    literals,
                    *min,
                    *max,
                    boolean_indices,
                )?);
            }
            Constraint::LinearComparison(comparison) => {
                model.constraints.push(linear_comparison_constraint(
                    comparison,
                    integer_indices,
                    &model.variables,
                    constant_one_index,
                )?);
            }
            _ => return Err(TranslationError::UnsupportedConstraint),
        }
        let generated = &mut model.constraints[first_generated..];
        if let Some((last, prior)) = generated.split_last_mut() {
            for constraint in prior {
                constraint.enforcement_literal.clone_from(&enforcement);
            }
            last.enforcement_literal = enforcement;
        }
    }
    Ok(())
}

fn translate_objective(
    problem: &PlanningProblem,
    weights: &[i64],
    model: &mut CpModelProto,
    integer_indices: &BTreeMap<IntVariableId, i32>,
    constant_one_index: i32,
) -> Result<(), TranslationError> {
    if problem.objectives.levels.is_empty() {
        return Ok(());
    }

    let mut combined = BTreeMap::<i32, i128>::new();
    let mut constant = 0_i128;
    for (level, &weight) in problem.objectives.levels.iter().zip(weights) {
        let signed_weight = match level.direction {
            OptimizationDirection::Minimize => i128::from(weight),
            OptimizationDirection::Maximize => -i128::from(weight),
        };
        for term in &level.terms {
            let weighted_constant = i128::from(term.expression.constant)
                .checked_mul(signed_weight)
                .ok_or(TranslationError::NativeIntegerOverflow)?;
            constant = constant
                .checked_add(weighted_constant)
                .ok_or(TranslationError::NativeIntegerOverflow)?;
            for expression_term in &term.expression.terms {
                let index = integer_indices
                    .get(&expression_term.variable)
                    .copied()
                    .ok_or(TranslationError::MissingIntegerIndex)?;
                let contribution = i128::from(expression_term.coefficient)
                    .checked_mul(signed_weight)
                    .ok_or(TranslationError::NativeIntegerOverflow)?;
                let coefficient = combined.entry(index).or_default();
                *coefficient = coefficient
                    .checked_add(contribution)
                    .ok_or(TranslationError::NativeIntegerOverflow)?;
            }
        }
    }
    if constant != 0 {
        combined.insert(constant_one_index, constant);
    }
    combined.retain(|_, coefficient| *coefficient != 0);

    let mut native_variables = Vec::with_capacity(combined.len());
    let mut coefficients = Vec::with_capacity(combined.len());
    for (variable, coefficient) in combined {
        native_variables.push(variable);
        coefficients
            .push(i64::try_from(coefficient).map_err(|_| TranslationError::NativeIntegerOverflow)?);
    }
    ensure_native_linear_safe(&native_variables, &coefficients, &model.variables)?;
    model.objective = Some(CpObjectiveProto {
        vars: native_variables,
        coeffs: coefficients,
        offset: 0.0,
        scaling_factor: 1.0,
        domain: Vec::new(),
        ..CpObjectiveProto::default()
    });
    Ok(())
}

fn linear_comparison_constraint(
    comparison: &LinearComparison,
    integer_indices: &BTreeMap<IntVariableId, i32>,
    variables: &[IntegerVariableProto],
    constant_one_index: i32,
) -> Result<ConstraintProto, TranslationError> {
    let has_constant = comparison.expression.constant != 0;
    let mut native_variables =
        Vec::with_capacity(comparison.expression.terms.len() + usize::from(has_constant));
    let mut coefficients = Vec::with_capacity(native_variables.capacity());

    if has_constant {
        native_variables.push(constant_one_index);
        coefficients.push(comparison.expression.constant);
    }
    for term in &comparison.expression.terms {
        native_variables.push(
            integer_indices
                .get(&term.variable)
                .copied()
                .ok_or(TranslationError::MissingIntegerIndex)?,
        );
        coefficients.push(term.coefficient);
    }
    ensure_native_linear_safe(&native_variables, &coefficients, variables)?;

    let domain = match comparison.op {
        ComparisonOp::Equal => vec![comparison.rhs, comparison.rhs],
        ComparisonOp::LessOrEqual => vec![i64::MIN, comparison.rhs],
        ComparisonOp::GreaterOrEqual => vec![comparison.rhs, i64::MAX],
    };
    Ok(ConstraintProto {
        name: String::new(),
        enforcement_literal: Vec::new(),
        constraint: Some(constraint_proto::Constraint::Linear(
            LinearConstraintProto {
                vars: native_variables,
                coeffs: coefficients,
                domain,
            },
        )),
    })
}

fn ensure_native_domain(domain: &IntDomain) -> Result<(), TranslationError> {
    let (lower, upper) = domain
        .bounds()
        .ok_or(TranslationError::NativeIntegerOverflow)?;
    let limit = i64::MAX / 2;
    if lower < -limit || upper > limit {
        return Err(TranslationError::NativeIntegerOverflow);
    }
    Ok(())
}

fn ensure_native_linear_safe(
    native_variables: &[i32],
    coefficients: &[i64],
    variables: &[IntegerVariableProto],
) -> Result<(), TranslationError> {
    let limit = i128::from(i64::MAX / 2);
    let mut conservative_min = 0_i128;
    let mut conservative_max = 0_i128;
    for (&variable, &coefficient) in native_variables.iter().zip(coefficients) {
        if coefficient == i64::MIN {
            return Err(TranslationError::NativeIntegerOverflow);
        }
        let variable = usize::try_from(variable)
            .ok()
            .and_then(|index| variables.get(index))
            .ok_or(TranslationError::NativeIntegerOverflow)?;
        let lower = i128::from(
            *variable
                .domain
                .first()
                .ok_or(TranslationError::NativeIntegerOverflow)?,
        );
        let upper = i128::from(
            *variable
                .domain
                .last()
                .ok_or(TranslationError::NativeIntegerOverflow)?,
        );
        let coefficient = i128::from(coefficient);
        let first = lower * coefficient;
        let second = upper * coefficient;
        conservative_min = conservative_min
            .checked_add(first.min(second).min(0))
            .ok_or(TranslationError::NativeIntegerOverflow)?;
        conservative_max = conservative_max
            .checked_add(first.max(second).max(0))
            .ok_or(TranslationError::NativeIntegerOverflow)?;
        if conservative_min < -limit || conservative_max > limit {
            return Err(TranslationError::NativeIntegerOverflow);
        }
    }
    Ok(())
}

fn cardinality_constraint(
    literals: &[Literal],
    min: u64,
    max: u64,
    boolean_indices: &BTreeMap<BoolVariableId, i32>,
) -> Result<ConstraintProto, TranslationError> {
    let mut coefficients = BTreeMap::<i32, i64>::new();
    let mut negative_count = 0_u64;
    for literal in literals {
        let index = boolean_indices
            .get(&literal.variable)
            .copied()
            .ok_or(TranslationError::MissingBooleanIndex)?;
        let coefficient = if literal.positive {
            1
        } else {
            negative_count = negative_count
                .checked_add(1)
                .ok_or(TranslationError::CardinalityBoundOverflow)?;
            -1
        };
        *coefficients.entry(index).or_default() += coefficient;
    }
    coefficients.retain(|_, coefficient| *coefficient != 0);

    let offset =
        i64::try_from(negative_count).map_err(|_| TranslationError::CardinalityBoundOverflow)?;
    let lower = i64::try_from(min)
        .ok()
        .and_then(|min| min.checked_sub(offset))
        .ok_or(TranslationError::CardinalityBoundOverflow)?;
    let upper = i64::try_from(max)
        .ok()
        .and_then(|max| max.checked_sub(offset))
        .ok_or(TranslationError::CardinalityBoundOverflow)?;
    let (vars, coeffs) = coefficients.into_iter().unzip();

    Ok(ConstraintProto {
        name: String::new(),
        enforcement_literal: Vec::new(),
        constraint: Some(constraint_proto::Constraint::Linear(
            LinearConstraintProto {
                vars,
                coeffs,
                domain: vec![lower, upper],
            },
        )),
    })
}

fn boolean_constraint(
    constructor: fn(BoolArgumentProto) -> constraint_proto::Constraint,
    literals: Vec<i32>,
) -> ConstraintProto {
    ConstraintProto {
        name: String::new(),
        enforcement_literal: Vec::new(),
        constraint: Some(constructor(BoolArgumentProto { literals })),
    }
}

fn translate_literal(
    literal: &Literal,
    boolean_indices: &BTreeMap<BoolVariableId, i32>,
) -> Result<i32, TranslationError> {
    let index = boolean_indices
        .get(&literal.variable)
        .copied()
        .ok_or(TranslationError::MissingBooleanIndex)?;
    Ok(if literal.positive { index } else { !index })
}

fn translate_literals(
    literals: &[Literal],
    boolean_indices: &BTreeMap<BoolVariableId, i32>,
) -> Result<Vec<i32>, TranslationError> {
    literals
        .iter()
        .map(|literal| translate_literal(literal, boolean_indices))
        .collect()
}

fn flatten_domain(domain: &IntDomain) -> Vec<i64> {
    let mut flattened = Vec::with_capacity(domain.inclusive_ranges.len() * 2);
    for range in &domain.inclusive_ranges {
        flattened.extend([range.start, range.end]);
    }
    flattened
}

#[cfg(test)]
mod tests {
    use super::*;
    use eutheto_domain_ir::ScoreCategoryId;
    use eutheto_planning_ir::{
        BoolVariable, Capability, CompilerId, ConstraintRecord, InclusiveRange, IntVariable,
        IntervalVariable, IntervalVariableId, LinearExpression, LinearTerm, ObjectiveLevel,
        ObjectiveLevelId, ObjectivePlan, ObjectiveTerm, ObjectiveTermId, ObjectiveTermKind,
        PLANNING_IR_SCHEMA_VERSION, PROJECTION_SCHEMA_VERSION, PlanningConstraintId,
        PlanningMetadata, ProvenanceId, ProvenanceRecord, ProvenanceSourceKind,
    };
    use eutheto_types::{PackId, ScenarioId};
    use std::collections::{BTreeMap, BTreeSet};
    use std::error::Error;

    fn planning_problem() -> Result<PlanningProblem, Box<dyn Error>> {
        let provenance = ProvenanceId::new("translation.variable")?;
        let bool_a = BoolVariableId::new("translation.a_bool")?;
        let int_start = IntVariableId::new("translation.b_start")?;
        let int_duration = IntVariableId::new("translation.c_duration")?;
        let int_end = IntVariableId::new("translation.d_end")?;
        let bool_z = BoolVariableId::new("translation.z_bool")?;
        let mut variables = vec![
            Variable::Boolean(BoolVariable {
                id: bool_z,
                provenance: provenance.clone(),
            }),
            Variable::Integer(IntVariable {
                id: int_end.clone(),
                domain: IntDomain::new(vec![InclusiveRange { start: 1, end: 4 }])?,
                provenance: provenance.clone(),
            }),
            Variable::Interval(IntervalVariable {
                id: IntervalVariableId::new("translation.interval")?,
                start: int_start.clone(),
                duration: int_duration.clone(),
                end: int_end,
                presence: None,
                provenance: provenance.clone(),
            }),
            Variable::Integer(IntVariable {
                id: int_duration,
                domain: IntDomain::new(vec![InclusiveRange { start: 1, end: 2 }])?,
                provenance: provenance.clone(),
            }),
            Variable::Boolean(BoolVariable {
                id: bool_a,
                provenance: provenance.clone(),
            }),
            Variable::Integer(IntVariable {
                id: int_start,
                domain: IntDomain::new(vec![
                    InclusiveRange { start: 0, end: 0 },
                    InclusiveRange { start: 2, end: 2 },
                ])?,
                provenance: provenance.clone(),
            }),
        ];
        variables.sort_by(|left, right| left.canonical_id().cmp(right.canonical_id()));
        Ok(PlanningProblem {
            schema_version: PLANNING_IR_SCHEMA_VERSION,
            variables,
            constraints: Vec::new(),
            objectives: ObjectivePlan::default(),
            assumptions: Vec::new(),
            projections: Vec::new(),
            provenance: vec![ProvenanceRecord {
                id: provenance,
                source_kind: ProvenanceSourceKind::Fact,
                source_id: "translation.fixture".to_owned(),
                entity_refs: Vec::new(),
                message_key: "translation.fixture".to_owned(),
                parameters: BTreeMap::new(),
                parent: None,
            }],
            metadata: PlanningMetadata {
                pack_id: PackId::new("official.synthetic")?,
                scenario_id: "01890a5d-ac96-7b64-9f74-bbfcf30f9f80".parse::<ScenarioId>()?,
                scenario_revision: 1,
                projection_version: PROJECTION_SCHEMA_VERSION,
                compiler_id: CompilerId::new("compiler.translation")?,
                compiler_version: "1.0.0".to_owned(),
                compile_metadata: BTreeMap::new(),
                display_text: BTreeMap::new(),
            },
            declared_capabilities: BTreeSet::new(),
            split_authorization: None,
        })
    }
    fn scalar_problem() -> Result<PlanningProblem, Box<dyn Error>> {
        let mut problem = planning_problem()?;
        problem
            .variables
            .retain(|variable| !matches!(variable, Variable::Interval(_)));
        Ok(problem)
    }

    fn constraint_record(
        id: &str,
        body: Constraint,
        enforcement: Vec<Literal>,
    ) -> Result<ConstraintRecord, Box<dyn Error>> {
        Ok(ConstraintRecord {
            id: PlanningConstraintId::new(id)?,
            body,
            enforcement,
            provenance: ProvenanceId::new("translation.variable")?,
            tags: Vec::new(),
        })
    }
    fn objective_term(
        id: &str,
        expression: LinearExpression,
        kind: ObjectiveTermKind,
    ) -> Result<ObjectiveTerm, Box<dyn Error>> {
        Ok(ObjectiveTerm {
            id: ObjectiveTermId::new(id)?,
            expression,
            kind,
            category: ScoreCategoryId::new("translation.score")?,
            provenance: ProvenanceId::new("translation.variable")?,
        })
    }

    #[test]
    fn boolean_and_integer_domains_follow_canonical_scalar_order() -> Result<(), Box<dyn Error>> {
        let problem = planning_problem()?;
        let translated = translate_variable_domains(&problem, PlanningIrLimitsV1::DEFAULT)?;

        assert_eq!(translated.variable_count(), 5);
        assert_eq!(
            translated.boolean_index(&BoolVariableId::new("translation.a_bool")?),
            Some(0)
        );
        assert_eq!(
            translated.integer_index(&IntVariableId::new("translation.b_start")?),
            Some(1)
        );
        assert_eq!(
            translated.integer_index(&IntVariableId::new("translation.c_duration")?),
            Some(2)
        );
        assert_eq!(
            translated.integer_index(&IntVariableId::new("translation.d_end")?),
            Some(3)
        );
        assert_eq!(
            translated.boolean_index(&BoolVariableId::new("translation.z_bool")?),
            Some(4)
        );
        assert_eq!(translated.model.variables[0].domain, BOOLEAN_DOMAIN);
        assert_eq!(translated.model.variables[1].domain, [0, 0, 2, 2]);
        assert_eq!(translated.model.variables[2].domain, [1, 2]);
        assert_eq!(translated.model.variables[3].domain, [1, 4]);
        assert_eq!(translated.model.variables[4].domain, BOOLEAN_DOMAIN);
        assert!(
            translated
                .model
                .variables
                .iter()
                .all(|variable| variable.name.is_empty())
        );
        Ok(())
    }

    #[test]
    fn invalid_planning_ir_is_rejected_before_translation() -> Result<(), Box<dyn Error>> {
        let mut problem = planning_problem()?;
        problem.variables.swap(0, 1);

        assert!(matches!(
            translate_variable_domains(&problem, PlanningIrLimitsV1::DEFAULT),
            Err(TranslationError::InvalidPlanningIr(_))
        ));
        Ok(())
    }

    #[test]
    fn translation_is_deterministic_and_contains_no_model_text() -> Result<(), Box<dyn Error>> {
        let problem = planning_problem()?;
        let first = translate_variable_domains(&problem, PlanningIrLimitsV1::DEFAULT)?;
        let second = translate_variable_domains(&problem, PlanningIrLimitsV1::DEFAULT)?;

        assert_eq!(first.model, second.model);
        assert!(first.model.name.is_empty());
        assert!(first.model.constraints.is_empty());
        Ok(())
    }

    #[test]
    fn bool_or_clauses_preserve_exact_cp_sat_literal_semantics() -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let bool_a = BoolVariableId::new("translation.a_bool")?;
        let bool_z = BoolVariableId::new("translation.z_bool")?;
        problem.constraints = vec![
            constraint_record(
                "constraint.a_empty",
                Constraint::bool_or(Vec::new()),
                Vec::new(),
            )?,
            constraint_record(
                "constraint.z_mixed",
                Constraint::bool_or(vec![
                    Literal::positive(bool_a.clone()),
                    Literal::negative(bool_z.clone()),
                ]),
                Vec::new(),
            )?,
        ];
        problem.declared_capabilities.insert(Capability::BoolOr);

        let translated = translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT)?;
        let model = CpModelProto::decode(translated.cp_model_proto())?;

        assert_eq!(translated.boolean_index(&bool_a), Some(0));
        assert_eq!(translated.boolean_index(&bool_z), Some(4));
        assert_eq!(model.constraints.len(), 2);
        let empty =
            model.constraints[0]
                .constraint
                .as_ref()
                .and_then(|constraint| match constraint {
                    constraint_proto::Constraint::BoolOr(argument) => Some(&argument.literals),
                    _ => None,
                });
        assert_eq!(empty, Some(&Vec::new()));
        let mixed =
            model.constraints[1]
                .constraint
                .as_ref()
                .and_then(|constraint| match constraint {
                    constraint_proto::Constraint::BoolOr(argument) => Some(&argument.literals),
                    _ => None,
                });
        assert_eq!(mixed, Some(&vec![0, -5]));
        assert!(model.name.is_empty());
        assert!(model.constraints.iter().all(|constraint| {
            constraint.name.is_empty() && constraint.enforcement_literal.is_empty()
        }));
        assert!(
            !String::from_utf8_lossy(translated.cp_model_proto()).contains("translation"),
            "planning IDs and provenance must not enter native model bytes"
        );
        Ok(())
    }

    #[test]
    fn boolean_relations_have_exact_clause_encodings() -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let bool_a = BoolVariableId::new("translation.a_bool")?;
        let bool_z = BoolVariableId::new("translation.z_bool")?;
        problem.constraints = vec![
            constraint_record(
                "constraint.a_empty_and",
                Constraint::bool_and(Vec::new()),
                Vec::new(),
            )?,
            constraint_record(
                "constraint.b_mixed_and",
                Constraint::bool_and(vec![
                    Literal::positive(bool_a.clone()),
                    Literal::negative(bool_z.clone()),
                ]),
                Vec::new(),
            )?,
            constraint_record(
                "constraint.c_implication",
                Constraint::Implication {
                    antecedent: Literal::positive(bool_a.clone()),
                    consequent: Literal::negative(bool_z.clone()),
                },
                Vec::new(),
            )?,
            constraint_record(
                "constraint.d_equivalence",
                Constraint::Equivalence {
                    left: Literal::negative(bool_a),
                    right: Literal::positive(bool_z),
                },
                Vec::new(),
            )?,
        ];
        problem.declared_capabilities.extend([
            Capability::BoolAnd,
            Capability::Implication,
            Capability::Equivalence,
        ]);

        let translated = translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT)?;
        let model = CpModelProto::decode(translated.cp_model_proto())?;
        let constraints: Vec<_> = model
            .constraints
            .iter()
            .map(|constraint| match constraint.constraint.as_ref() {
                Some(constraint_proto::Constraint::BoolOr(argument)) => {
                    ("or", argument.literals.as_slice())
                }
                Some(constraint_proto::Constraint::BoolAnd(argument)) => {
                    ("and", argument.literals.as_slice())
                }
                _ => ("other", &[][..]),
            })
            .collect();

        assert_eq!(
            constraints,
            [
                ("and", &[][..]),
                ("and", &[0, -5][..]),
                ("or", &[-1, -5][..]),
                ("or", &[0, 4][..]),
                ("or", &[-1, -5][..]),
            ]
        );
        Ok(())
    }

    #[test]
    fn one_of_constraints_preserve_empty_and_literal_semantics() -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let bool_a = BoolVariableId::new("translation.a_bool")?;
        let bool_z = BoolVariableId::new("translation.z_bool")?;
        problem.constraints = vec![
            constraint_record(
                "constraint.a_empty_at_most",
                Constraint::at_most_one(Vec::new()),
                Vec::new(),
            )?,
            constraint_record(
                "constraint.b_mixed_at_most",
                Constraint::at_most_one(vec![
                    Literal::positive(bool_a.clone()),
                    Literal::negative(bool_z.clone()),
                ]),
                Vec::new(),
            )?,
            constraint_record(
                "constraint.c_empty_exactly",
                Constraint::exactly_one(Vec::new()),
                Vec::new(),
            )?,
            constraint_record(
                "constraint.d_mixed_exactly",
                Constraint::exactly_one(vec![Literal::negative(bool_a), Literal::positive(bool_z)]),
                Vec::new(),
            )?,
        ];
        problem
            .declared_capabilities
            .extend([Capability::AtMostOne, Capability::ExactlyOne]);

        let translated = translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT)?;
        let model = CpModelProto::decode(translated.cp_model_proto())?;
        let constraints: Vec<_> = model
            .constraints
            .iter()
            .map(|constraint| match constraint.constraint.as_ref() {
                Some(constraint_proto::Constraint::AtMostOne(argument)) => {
                    ("at_most", argument.literals.as_slice())
                }
                Some(constraint_proto::Constraint::ExactlyOne(argument)) => {
                    ("exactly", argument.literals.as_slice())
                }
                _ => ("other", &[][..]),
            })
            .collect();

        assert_eq!(
            constraints,
            [
                ("at_most", &[][..]),
                ("at_most", &[0, -5][..]),
                ("exactly", &[][..]),
                ("exactly", &[-1, 4][..]),
            ]
        );
        Ok(())
    }

    #[test]
    fn cardinality_ranges_shift_negative_literals_and_combine_variables()
    -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let bool_a = BoolVariableId::new("translation.a_bool")?;
        let bool_z = BoolVariableId::new("translation.z_bool")?;
        problem.constraints = vec![
            constraint_record(
                "constraint.a_empty",
                Constraint::cardinality(Vec::new(), 0, 0)?,
                Vec::new(),
            )?,
            constraint_record(
                "constraint.b_mixed",
                Constraint::cardinality(
                    vec![
                        Literal::positive(bool_a.clone()),
                        Literal::negative(bool_z.clone()),
                    ],
                    1,
                    1,
                )?,
                Vec::new(),
            )?,
            constraint_record(
                "constraint.c_complementary",
                Constraint::cardinality(
                    vec![Literal::positive(bool_a.clone()), Literal::negative(bool_a)],
                    1,
                    1,
                )?,
                Vec::new(),
            )?,
            constraint_record(
                "constraint.d_negative",
                Constraint::cardinality(vec![Literal::negative(bool_z)], 0, 1)?,
                Vec::new(),
            )?,
        ];
        problem
            .declared_capabilities
            .insert(Capability::CardinalityRange);

        let translated = translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT)?;
        let model = CpModelProto::decode(translated.cp_model_proto())?;
        let constraints: Vec<_> = model
            .constraints
            .iter()
            .map(|constraint| match constraint.constraint.as_ref() {
                Some(constraint_proto::Constraint::Linear(linear)) => (
                    linear.vars.as_slice(),
                    linear.coeffs.as_slice(),
                    linear.domain.as_slice(),
                ),
                _ => (&[][..], &[][..], &[][..]),
            })
            .collect();

        assert_eq!(
            constraints,
            [
                (&[][..], &[][..], &[0, 0][..]),
                (&[0, 4][..], &[1, -1][..], &[0, 0][..]),
                (&[][..], &[][..], &[0, 0][..]),
                (&[4][..], &[-1][..], &[-1, 0][..]),
            ]
        );
        Ok(())
    }

    #[test]
    fn integer_linear_comparisons_preserve_constants_coefficients_and_bounds()
    -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let start = IntVariableId::new("translation.b_start")?;
        let duration = IntVariableId::new("translation.c_duration")?;
        let end = IntVariableId::new("translation.d_end")?;
        problem.constraints = vec![
            constraint_record(
                "constraint.a_equal",
                Constraint::LinearComparison(LinearComparison {
                    expression: LinearExpression::new(
                        vec![
                            LinearTerm {
                                variable: start.clone(),
                                coefficient: 2,
                            },
                            LinearTerm {
                                variable: duration,
                                coefficient: -3,
                            },
                        ],
                        7,
                    )?,
                    op: ComparisonOp::Equal,
                    rhs: 5,
                }),
                Vec::new(),
            )?,
            constraint_record(
                "constraint.b_less",
                Constraint::LinearComparison(LinearComparison {
                    expression: LinearExpression::new(
                        vec![LinearTerm {
                            variable: end,
                            coefficient: 1,
                        }],
                        0,
                    )?,
                    op: ComparisonOp::LessOrEqual,
                    rhs: 4,
                }),
                Vec::new(),
            )?,
            constraint_record(
                "constraint.c_greater",
                Constraint::LinearComparison(LinearComparison {
                    expression: LinearExpression::new(
                        vec![LinearTerm {
                            variable: start.clone(),
                            coefficient: -1,
                        }],
                        -2,
                    )?,
                    op: ComparisonOp::GreaterOrEqual,
                    rhs: -5,
                }),
                Vec::new(),
            )?,
        ];
        problem
            .declared_capabilities
            .insert(Capability::LinearComparison);

        let translated = translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT)?;
        let model = CpModelProto::decode(translated.cp_model_proto())?;
        let constraints: Vec<_> = model
            .constraints
            .iter()
            .map(|constraint| match constraint.constraint.as_ref() {
                Some(constraint_proto::Constraint::Linear(linear)) => (
                    linear.vars.as_slice(),
                    linear.coeffs.as_slice(),
                    linear.domain.as_slice(),
                ),
                _ => (&[][..], &[][..], &[][..]),
            })
            .collect();

        assert_eq!(model.variables.len(), 6);
        assert_eq!(model.variables[0].domain, [1, 1]);
        assert_eq!(translated.integer_index(&start), Some(2));
        assert_eq!(
            constraints,
            [
                (&[0, 2, 3][..], &[7, 2, -3][..], &[5, 5][..]),
                (&[4][..], &[1][..], &[i64::MIN, 4][..]),
                (&[0, 2][..], &[-2, -1][..], &[-5, i64::MAX][..]),
            ]
        );
        Ok(())
    }

    #[test]
    fn bounded_objective_terms_preserve_coefficients_constants_and_kinds()
    -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let start = IntVariableId::new("translation.b_start")?;
        let duration = IntVariableId::new("translation.c_duration")?;
        problem.objectives = ObjectivePlan {
            levels: vec![ObjectiveLevel {
                id: ObjectiveLevelId::new("translation.level")?,
                direction: OptimizationDirection::Minimize,
                lower_bound: 4,
                upper_bound: 9,
                terms: vec![
                    objective_term(
                        "translation.objective.a",
                        LinearExpression::new(
                            vec![LinearTerm {
                                variable: start.clone(),
                                coefficient: 2,
                            }],
                            3,
                        )?,
                        ObjectiveTermKind::Penalty,
                    )?,
                    objective_term(
                        "translation.objective.b",
                        LinearExpression::new(
                            vec![LinearTerm {
                                variable: duration,
                                coefficient: 1,
                            }],
                            0,
                        )?,
                        ObjectiveTermKind::Reward,
                    )?,
                ],
                provenance: ProvenanceId::new("translation.variable")?,
            }],
        };
        problem
            .declared_capabilities
            .extend([Capability::ObjectivePenalty, Capability::ObjectiveReward]);

        let translated = translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT)?;
        let model = CpModelProto::decode(translated.cp_model_proto())?;
        let objective = model.objective.as_ref().ok_or("missing objective")?;

        assert_eq!(model.variables[0].domain, [1, 1]);
        assert_eq!(translated.integer_index(&start), Some(2));
        assert_eq!(objective.vars, [0, 2, 3]);
        assert_eq!(objective.coeffs, [3, 2, 1]);
        assert_eq!(objective.offset.to_bits(), 0.0_f64.to_bits());
        assert_eq!(objective.scaling_factor.to_bits(), 1.0_f64.to_bits());
        Ok(())
    }

    #[test]
    fn exact_lexicographic_objective_normalizes_mixed_directions() -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let start = IntVariableId::new("translation.b_start")?;
        let duration = IntVariableId::new("translation.c_duration")?;
        problem.objectives = ObjectivePlan {
            levels: vec![
                ObjectiveLevel {
                    id: ObjectiveLevelId::new("translation.level.high")?,
                    direction: OptimizationDirection::Minimize,
                    lower_bound: 0,
                    upper_bound: 2,
                    terms: vec![objective_term(
                        "translation.objective.high",
                        LinearExpression::new(
                            vec![LinearTerm {
                                variable: start.clone(),
                                coefficient: 1,
                            }],
                            0,
                        )?,
                        ObjectiveTermKind::Penalty,
                    )?],
                    provenance: ProvenanceId::new("translation.variable")?,
                },
                ObjectiveLevel {
                    id: ObjectiveLevelId::new("translation.level.low")?,
                    direction: OptimizationDirection::Maximize,
                    lower_bound: 1,
                    upper_bound: 2,
                    terms: vec![objective_term(
                        "translation.objective.low",
                        LinearExpression::new(
                            vec![LinearTerm {
                                variable: duration,
                                coefficient: 1,
                            }],
                            0,
                        )?,
                        ObjectiveTermKind::Reward,
                    )?],
                    provenance: ProvenanceId::new("translation.variable")?,
                },
            ],
        };
        problem
            .declared_capabilities
            .extend([Capability::ObjectivePenalty, Capability::ObjectiveReward]);

        let translated = translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT)?;
        let model = CpModelProto::decode(translated.cp_model_proto())?;
        let objective = model.objective.as_ref().ok_or("missing objective")?;

        assert_eq!(translated.integer_index(&start), Some(1));
        assert_eq!(objective.vars, [1, 2]);
        assert_eq!(objective.coeffs, [2, -1]);
        Ok(())
    }

    #[test]
    fn objective_requiring_multipass_is_rejected_before_dispatch() -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let start = IntVariableId::new("translation.b_start")?;
        for variable in &mut problem.variables {
            if let Variable::Integer(integer) = variable
                && integer.id == start
            {
                integer.domain = IntDomain::new(vec![InclusiveRange {
                    start: 0,
                    end: 1_000_000,
                }])?;
            }
        }
        problem.objectives = ObjectivePlan {
            levels: (0..4)
                .map(|index| {
                    Ok(ObjectiveLevel {
                        id: ObjectiveLevelId::new(format!("translation.level.{index}"))?,
                        direction: OptimizationDirection::Minimize,
                        lower_bound: 0,
                        upper_bound: 1_000_000,
                        terms: vec![objective_term(
                            &format!("translation.objective.{index}"),
                            LinearExpression::new(
                                vec![LinearTerm {
                                    variable: start.clone(),
                                    coefficient: 1,
                                }],
                                0,
                            )?,
                            ObjectiveTermKind::Penalty,
                        )?],
                        provenance: ProvenanceId::new("translation.variable")?,
                    })
                })
                .collect::<Result<Vec<_>, Box<dyn Error>>>()?,
        };
        problem
            .declared_capabilities
            .insert(Capability::ObjectivePenalty);

        assert!(matches!(
            translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT),
            Err(TranslationError::UnsupportedMultipassObjective)
        ));
        Ok(())
    }

    #[test]
    fn native_conservative_objective_overflow_is_rejected_before_dispatch()
    -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let start = IntVariableId::new("translation.b_start")?;
        let duration = IntVariableId::new("translation.c_duration")?;
        for variable in &mut problem.variables {
            if let Variable::Integer(integer) = variable {
                if integer.id == start {
                    integer.domain = IntDomain::new(vec![InclusiveRange {
                        start: 0,
                        end: 3_000_000_000,
                    }])?;
                } else if integer.id == duration {
                    integer.domain = IntDomain::new(vec![InclusiveRange {
                        start: 0,
                        end: 2_000_000_000,
                    }])?;
                }
            }
        }
        problem.objectives = ObjectivePlan {
            levels: vec![
                ObjectiveLevel {
                    id: ObjectiveLevelId::new("translation.level.high")?,
                    direction: OptimizationDirection::Minimize,
                    lower_bound: 0,
                    upper_bound: 3_000_000_000,
                    terms: vec![objective_term(
                        "translation.objective.high",
                        LinearExpression::new(
                            vec![LinearTerm {
                                variable: start,
                                coefficient: 1,
                            }],
                            0,
                        )?,
                        ObjectiveTermKind::Penalty,
                    )?],
                    provenance: ProvenanceId::new("translation.variable")?,
                },
                ObjectiveLevel {
                    id: ObjectiveLevelId::new("translation.level.low")?,
                    direction: OptimizationDirection::Minimize,
                    lower_bound: 0,
                    upper_bound: 2_000_000_000,
                    terms: vec![objective_term(
                        "translation.objective.low",
                        LinearExpression::new(
                            vec![LinearTerm {
                                variable: duration,
                                coefficient: 1,
                            }],
                            0,
                        )?,
                        ObjectiveTermKind::Penalty,
                    )?],
                    provenance: ProvenanceId::new("translation.variable")?,
                },
            ],
        };
        problem
            .declared_capabilities
            .insert(Capability::ObjectivePenalty);

        assert!(matches!(
            translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT),
            Err(TranslationError::NativeIntegerOverflow)
        ));
        Ok(())
    }

    #[test]
    fn minimum_scalarized_objective_coefficient_is_rejected_before_dispatch()
    -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let start = IntVariableId::new("translation.b_start")?;
        let duration = IntVariableId::new("translation.c_duration")?;
        let end = IntVariableId::new("translation.d_end")?;
        for variable in &mut problem.variables {
            if let Variable::Integer(integer) = variable {
                if integer.id == start || integer.id == duration {
                    integer.domain = IntDomain::new(vec![InclusiveRange {
                        start: 0,
                        end: i64::from(i32::MAX),
                    }])?;
                } else if integer.id == end {
                    integer.domain = IntDomain::new(vec![InclusiveRange { start: 0, end: 0 }])?;
                }
            }
        }
        problem.objectives = ObjectivePlan {
            levels: vec![
                ObjectiveLevel {
                    id: ObjectiveLevelId::new("translation.level.high")?,
                    direction: OptimizationDirection::Maximize,
                    lower_bound: 0,
                    upper_bound: 0,
                    terms: vec![objective_term(
                        "translation.objective.high",
                        LinearExpression::new(
                            vec![LinearTerm {
                                variable: end,
                                coefficient: 2,
                            }],
                            0,
                        )?,
                        ObjectiveTermKind::Reward,
                    )?],
                    provenance: ProvenanceId::new("translation.variable")?,
                },
                ObjectiveLevel {
                    id: ObjectiveLevelId::new("translation.level.middle")?,
                    direction: OptimizationDirection::Minimize,
                    lower_bound: 0,
                    upper_bound: i64::from(i32::MAX),
                    terms: vec![objective_term(
                        "translation.objective.middle",
                        LinearExpression::new(
                            vec![LinearTerm {
                                variable: start,
                                coefficient: 1,
                            }],
                            0,
                        )?,
                        ObjectiveTermKind::Penalty,
                    )?],
                    provenance: ProvenanceId::new("translation.variable")?,
                },
                ObjectiveLevel {
                    id: ObjectiveLevelId::new("translation.level.low")?,
                    direction: OptimizationDirection::Minimize,
                    lower_bound: 0,
                    upper_bound: i64::from(i32::MAX),
                    terms: vec![objective_term(
                        "translation.objective.low",
                        LinearExpression::new(
                            vec![LinearTerm {
                                variable: duration,
                                coefficient: 1,
                            }],
                            0,
                        )?,
                        ObjectiveTermKind::Penalty,
                    )?],
                    provenance: ProvenanceId::new("translation.variable")?,
                },
            ],
        };
        problem
            .declared_capabilities
            .extend([Capability::ObjectivePenalty, Capability::ObjectiveReward]);

        assert!(matches!(
            translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT),
            Err(TranslationError::NativeIntegerOverflow)
        ));
        Ok(())
    }

    #[test]
    fn native_conservative_linear_overflow_is_rejected_before_dispatch()
    -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let start = IntVariableId::new("translation.b_start")?;
        for variable in &mut problem.variables {
            if let Variable::Integer(integer) = variable
                && integer.id == start
            {
                integer.domain = IntDomain::new(vec![InclusiveRange {
                    start: 0,
                    end: 5_000_000,
                }])?;
            }
        }
        problem.constraints.push(constraint_record(
            "constraint.native_overflow",
            Constraint::LinearComparison(LinearComparison {
                expression: LinearExpression::new(
                    vec![LinearTerm {
                        variable: start,
                        coefficient: 1_000_000_000_000,
                    }],
                    0,
                )?,
                op: ComparisonOp::LessOrEqual,
                rhs: 0,
            }),
            Vec::new(),
        )?);
        problem
            .declared_capabilities
            .insert(Capability::LinearComparison);

        assert!(matches!(
            translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT),
            Err(TranslationError::NativeIntegerOverflow)
        ));
        Ok(())
    }

    #[test]
    fn unsupported_constraint_semantics_are_rejected_not_omitted() -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        problem.constraints.push(constraint_record(
            "constraint.unsupported",
            Constraint::all_different(Vec::new()),
            Vec::new(),
        )?);
        problem
            .declared_capabilities
            .insert(Capability::AllDifferent);

        assert!(matches!(
            translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT),
            Err(TranslationError::UnsupportedConstraint)
        ));
        Ok(())
    }

    #[test]
    fn enforcement_literals_apply_to_every_generated_native_constraint()
    -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let bool_a = BoolVariableId::new("translation.a_bool")?;
        let bool_z = BoolVariableId::new("translation.z_bool")?;
        problem.constraints = vec![
            constraint_record(
                "constraint.a_enforced_clause",
                Constraint::bool_or(vec![Literal::positive(bool_z.clone())]),
                vec![
                    Literal::positive(bool_a.clone()),
                    Literal::negative(bool_z.clone()),
                ],
            )?,
            constraint_record(
                "constraint.b_enforced_equivalence",
                Constraint::Equivalence {
                    left: Literal::positive(bool_a.clone()),
                    right: Literal::positive(bool_z.clone()),
                },
                vec![Literal::negative(bool_a.clone())],
            )?,
            constraint_record(
                "constraint.c_enforced_at_most",
                Constraint::at_most_one(vec![
                    Literal::positive(bool_a.clone()),
                    Literal::positive(bool_z.clone()),
                ]),
                vec![Literal::positive(bool_z.clone())],
            )?,
            constraint_record(
                "constraint.d_enforced_exactly",
                Constraint::exactly_one(vec![
                    Literal::positive(bool_a),
                    Literal::positive(bool_z.clone()),
                ]),
                vec![Literal::negative(bool_z)],
            )?,
        ];
        problem.declared_capabilities.extend([
            Capability::BoolOr,
            Capability::Equivalence,
            Capability::AtMostOne,
            Capability::ExactlyOne,
        ]);

        let translated = translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT)?;
        let model = CpModelProto::decode(translated.cp_model_proto())?;
        let enforcement: Vec<_> = model
            .constraints
            .iter()
            .map(|constraint| constraint.enforcement_literal.as_slice())
            .collect();

        assert_eq!(
            enforcement,
            [&[0, -5][..], &[-1][..], &[-1][..], &[4][..], &[-5][..],]
        );
        assert!(model.constraints[3..].iter().all(|constraint| matches!(
            constraint.constraint,
            Some(constraint_proto::Constraint::Linear(_))
        )));
        Ok(())
    }
}
