use std::collections::BTreeMap;

use eutheto_planning_ir::{
    BoolVariableId, IntDomain, IntVariableId, PlanningIrLimitsV1, PlanningProblem, ValidationError,
    Variable, validate,
};
use thiserror::Error;

use crate::cp_sat::{CpModelProto, IntegerVariableProto};

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

/// Safe failures before a variable declaration can enter a worker request.
#[derive(Debug, Error)]
pub enum VariableTranslationError {
    /// The solver-neutral model did not satisfy its complete bounded schema contract.
    #[error("planning IR validation failed before CP-SAT variable translation")]
    InvalidPlanningIr(#[from] ValidationError),
    /// CP-SAT variable references are signed 32-bit indices.
    #[error("the CP-SAT scalar variable index exceeds the signed 32-bit protocol range")]
    VariableIndexOverflow,
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
) -> Result<VariableTranslation, VariableTranslationError> {
    validate(problem, limits)?;

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
            .map_err(|_| VariableTranslationError::VariableIndexOverflow)?;
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
        BoolVariable, CompilerId, InclusiveRange, IntVariable, IntervalVariable,
        IntervalVariableId, ObjectivePlan, PLANNING_IR_SCHEMA_VERSION, PROJECTION_SCHEMA_VERSION,
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
            Err(VariableTranslationError::InvalidPlanningIr(_))
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
}
