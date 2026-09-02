use std::collections::BTreeMap;

use eutheto_planning_ir::{
    BoolVariableId, Constraint, IntDomain, IntVariableId, Literal, PlanningIrLimitsV1,
    PlanningProblem, ValidationError, Variable, validate,
};
use prost::Message;
use thiserror::Error;

use crate::cp_sat::{
    BoolArgumentProto, ConstraintProto, CpModelProto, IntegerVariableProto, constraint_proto,
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
    /// This incremental translator does not yet encode this constraint primitive.
    #[error("the planning constraint is not supported by the current CP-SAT translator")]
    UnsupportedConstraint,
    /// Common constraint enforcement is translated in a later stage.
    #[error("constraint enforcement is not supported by the current CP-SAT translator")]
    UnsupportedEnforcement,
    /// Objectives are translated in a later stage.
    #[error("objectives are not supported by the current CP-SAT translator")]
    UnsupportedObjective,
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
    translate_variable_domains_validated(problem)
}

fn translate_variable_domains_validated(
    problem: &PlanningProblem,
) -> Result<VariableTranslation, TranslationError> {
    let scalar_count = problem
        .variables
        .iter()
        .filter(|variable| !matches!(variable, Variable::Interval(_)))
        .count();
    let mut model = CpModelProto {
        variables: Vec::with_capacity(scalar_count),
        ..CpModelProto::default()
    };
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
/// At this stage the accepted model surface is Boolean and integer scalar variables plus
/// unenforced `BoolOr` clauses. Unsupported primitives are rejected rather than omitted.
/// Positive literals use their scalar variable index; negative literals use CP-SAT's exact
/// `-index - 1` encoding. Empty clauses remain empty and therefore false.
///
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
    if !problem.objectives.levels.is_empty() {
        return Err(TranslationError::UnsupportedObjective);
    }
    if !problem.assumptions.is_empty() {
        return Err(TranslationError::UnsupportedAssumption);
    }
    if !problem.projections.is_empty() {
        return Err(TranslationError::UnsupportedProjection);
    }

    let VariableTranslation {
        mut model,
        boolean_indices,
        integer_indices,
    } = translate_variable_domains_validated(problem)?;
    model.constraints.reserve(problem.constraints.len());

    for record in &problem.constraints {
        if !record.enforcement.is_empty() {
            return Err(TranslationError::UnsupportedEnforcement);
        }
        let Constraint::BoolOr { literals } = &record.body else {
            return Err(TranslationError::UnsupportedConstraint);
        };
        model.constraints.push(ConstraintProto {
            name: String::new(),
            enforcement_literal: Vec::new(),
            constraint: Some(constraint_proto::Constraint::BoolOr(BoolArgumentProto {
                literals: translate_literals(literals, &boolean_indices)?,
            })),
        });
    }

    Ok(TranslatedCpSatModel {
        cp_model_proto: model.encode_to_vec(),
        boolean_indices,
        integer_indices,
    })
}

fn translate_literals(
    literals: &[Literal],
    boolean_indices: &BTreeMap<BoolVariableId, i32>,
) -> Result<Vec<i32>, TranslationError> {
    literals
        .iter()
        .map(|literal| {
            let index = boolean_indices
                .get(&literal.variable)
                .copied()
                .ok_or(TranslationError::MissingBooleanIndex)?;
            if literal.positive {
                Ok(index)
            } else {
                Ok(-index - 1)
            }
        })
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
    use eutheto_planning_ir::{
        BoolVariable, Capability, CompilerId, ConstraintRecord, InclusiveRange, IntVariable,
        IntervalVariable, IntervalVariableId, ObjectivePlan, PLANNING_IR_SCHEMA_VERSION,
        PROJECTION_SCHEMA_VERSION, PlanningConstraintId, PlanningMetadata, ProvenanceId,
        ProvenanceRecord, ProvenanceSourceKind,
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
    fn unsupported_constraint_semantics_are_rejected_not_omitted() -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        problem.constraints.push(constraint_record(
            "constraint.unsupported",
            Constraint::bool_and(Vec::new()),
            Vec::new(),
        )?);
        problem.declared_capabilities.insert(Capability::BoolAnd);

        assert!(matches!(
            translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT),
            Err(TranslationError::UnsupportedConstraint)
        ));
        Ok(())
    }

    #[test]
    fn unsupported_enforcement_is_rejected_not_omitted() -> Result<(), Box<dyn Error>> {
        let mut problem = scalar_problem()?;
        let bool_a = BoolVariableId::new("translation.a_bool")?;
        problem.constraints.push(constraint_record(
            "constraint.enforced",
            Constraint::bool_or(Vec::new()),
            vec![Literal::positive(bool_a)],
        )?);
        problem.declared_capabilities.insert(Capability::BoolOr);

        assert!(matches!(
            translate_supported_model(&problem, PlanningIrLimitsV1::DEFAULT),
            Err(TranslationError::UnsupportedEnforcement)
        ));
        Ok(())
    }
}
