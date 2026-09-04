use eutheto_domain_ir::ConflictGroupV1;
use eutheto_planning_ir::{
    Literal, PlanningIrLimitsV1, PlanningProblem, ValidationError, validate,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A strict failure while mapping backend-returned assumption literals.
#[derive(Debug)]
pub enum AssumptionMappingError {
    /// The planning problem is not valid canonical IR.
    InvalidProblem(ValidationError),
    /// A conflict must contain at least one literal.
    Empty,
    /// The returned set exceeds the caller's assumption bound.
    LimitExceeded,
    /// The backend returned the same literal more than once.
    DuplicateLiteral(Literal),
    /// No assumption uses the returned Boolean variable.
    UnknownLiteral(Literal),
    /// An assumption uses the variable, but with the opposite polarity.
    WrongPolarity(Literal),
}

impl fmt::Display for AssumptionMappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProblem(error) => write!(formatter, "invalid planning problem: {error}"),
            Self::Empty => formatter.write_str("assumption conflict is empty"),
            Self::LimitExceeded => formatter.write_str("assumption conflict exceeds its limit"),
            Self::DuplicateLiteral(_) => {
                formatter.write_str("assumption conflict contains a duplicate literal")
            }
            Self::UnknownLiteral(_) => {
                formatter.write_str("assumption conflict contains an unknown literal")
            }
            Self::WrongPolarity(_) => formatter
                .write_str("assumption conflict contains a literal with the wrong polarity"),
        }
    }
}

impl std::error::Error for AssumptionMappingError {}

/// Maps one complete backend-returned literal set to canonical domain conflict groups.
///
/// The planning problem and the entire returned set are validated before any result is exposed.
/// Empty, oversized, duplicate, unknown, and wrong-polarity inputs have distinct errors. On any
/// error no partial group vector is returned.
///
/// # Errors
/// Returns [`AssumptionMappingError`] for an invalid problem or returned literal set.
pub fn map_assumption_literals(
    problem: &PlanningProblem,
    literals: &[Literal],
    limits: PlanningIrLimitsV1,
) -> Result<Vec<ConflictGroupV1>, AssumptionMappingError> {
    validate(problem, limits).map_err(AssumptionMappingError::InvalidProblem)?;
    if literals.is_empty() {
        return Err(AssumptionMappingError::Empty);
    }
    if u64::try_from(literals.len()).map_or(true, |count| count > limits.max_assumptions) {
        return Err(AssumptionMappingError::LimitExceeded);
    }

    let assumptions_by_literal = problem
        .assumptions
        .iter()
        .map(|assumption| (assumption.literal.clone(), assumption))
        .collect::<BTreeMap<_, _>>();
    let assumption_variables = problem
        .assumptions
        .iter()
        .map(|assumption| assumption.literal.variable.clone())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut mapped = Vec::with_capacity(literals.len());

    for literal in literals {
        if !seen.insert(literal.clone()) {
            return Err(AssumptionMappingError::DuplicateLiteral(literal.clone()));
        }
        let Some(assumption) = assumptions_by_literal.get(literal) else {
            return if assumption_variables.contains(&literal.variable) {
                Err(AssumptionMappingError::WrongPolarity(literal.clone()))
            } else {
                Err(AssumptionMappingError::UnknownLiteral(literal.clone()))
            };
        };
        mapped.push(ConflictGroupV1 {
            group_id: assumption.id.clone(),
            required_rules: assumption.required_rules.clone(),
        });
    }

    // Planning validation already proved unique group identities and non-empty canonical rule sets.
    mapped.sort();
    Ok(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eutheto_domain_ir::AssumptionGroupId;
    use eutheto_planning_ir::{
        Assumption, BoolVariable, BoolVariableId, Capability, CompilerId, ObjectivePlan,
        PLANNING_IR_SCHEMA_VERSION, PlanningMetadata, ProvenanceId, ProvenanceRecord,
        ProvenanceSourceKind, Variable,
    };
    use eutheto_types::{PackId, RuleId, ScenarioId};
    use std::collections::{BTreeMap, BTreeSet};

    fn problem() -> Result<PlanningProblem, Box<dyn std::error::Error>> {
        let left_variable = BoolVariableId::new("tests.left")?;
        let right_variable = BoolVariableId::new("tests.right")?;
        let left_provenance = ProvenanceId::new("tests.left")?;
        let right_provenance = ProvenanceId::new("tests.right")?;
        Ok(PlanningProblem {
            schema_version: PLANNING_IR_SCHEMA_VERSION,
            variables: vec![
                Variable::Boolean(BoolVariable {
                    id: left_variable.clone(),
                    provenance: left_provenance.clone(),
                }),
                Variable::Boolean(BoolVariable {
                    id: right_variable.clone(),
                    provenance: right_provenance.clone(),
                }),
            ],
            constraints: Vec::new(),
            objectives: ObjectivePlan::default(),
            assumptions: vec![
                Assumption {
                    id: AssumptionGroupId::new("tests.left")?,
                    literal: Literal::positive(left_variable),
                    required_rules: vec!["01890a5d-ac96-7b64-9f74-bbfcf30f9f01".parse::<RuleId>()?],
                    provenance: left_provenance.clone(),
                },
                Assumption {
                    id: AssumptionGroupId::new("tests.right")?,
                    literal: Literal::negative(right_variable),
                    required_rules: vec!["01890a5d-ac96-7b64-9f74-bbfcf30f9f02".parse::<RuleId>()?],
                    provenance: right_provenance.clone(),
                },
            ],
            projections: Vec::new(),
            provenance: vec![
                ProvenanceRecord {
                    id: left_provenance,
                    source_kind: ProvenanceSourceKind::RequiredRule,
                    source_id: "tests.left".to_owned(),
                    entity_refs: Vec::new(),
                    message_key: "tests.left".to_owned(),
                    parameters: BTreeMap::new(),
                    parent: None,
                },
                ProvenanceRecord {
                    id: right_provenance,
                    source_kind: ProvenanceSourceKind::RequiredRule,
                    source_id: "tests.right".to_owned(),
                    entity_refs: Vec::new(),
                    message_key: "tests.right".to_owned(),
                    parameters: BTreeMap::new(),
                    parent: None,
                },
            ],
            metadata: PlanningMetadata {
                pack_id: PackId::new("official.test")?,
                scenario_id: "01890a5d-ac96-7b64-9f74-bbfcf30f9f80".parse::<ScenarioId>()?,
                scenario_revision: 1,
                projection_version: 1,
                compiler_id: CompilerId::new("official.test")?,
                compiler_version: "1.0.0".to_owned(),
                compile_metadata: BTreeMap::new(),
                display_text: BTreeMap::new(),
            },
            declared_capabilities: BTreeSet::from([Capability::Assumptions]),
            split_authorization: None,
        })
    }

    #[test]
    fn maps_complete_set_to_canonical_typed_groups() -> Result<(), Box<dyn std::error::Error>> {
        let problem = problem()?;
        let groups = map_assumption_literals(
            &problem,
            &[
                problem.assumptions[1].literal.clone(),
                problem.assumptions[0].literal.clone(),
            ],
            PlanningIrLimitsV1::DEFAULT,
        )?;
        assert_eq!(groups[0].group_id, problem.assumptions[0].id);
        assert_eq!(groups[1].group_id, problem.assumptions[1].id);
        Ok(())
    }

    #[test]
    fn rejects_every_invalid_returned_literal_shape() -> Result<(), Box<dyn std::error::Error>> {
        let problem = problem()?;
        assert!(matches!(
            map_assumption_literals(&problem, &[], PlanningIrLimitsV1::DEFAULT),
            Err(AssumptionMappingError::Empty)
        ));
        let literal = problem.assumptions[0].literal.clone();
        assert!(matches!(
            map_assumption_literals(
                &problem,
                &[literal.clone(), literal],
                PlanningIrLimitsV1::DEFAULT,
            ),
            Err(AssumptionMappingError::DuplicateLiteral(_))
        ));
        assert!(matches!(
            map_assumption_literals(
                &problem,
                &[problem.assumptions[0].literal.negated()],
                PlanningIrLimitsV1::DEFAULT,
            ),
            Err(AssumptionMappingError::WrongPolarity(_))
        ));
        assert!(matches!(
            map_assumption_literals(
                &problem,
                &[Literal::positive(BoolVariableId::new("tests.unknown")?)],
                PlanningIrLimitsV1::DEFAULT,
            ),
            Err(AssumptionMappingError::UnknownLiteral(_))
        ));
        Ok(())
    }
}
