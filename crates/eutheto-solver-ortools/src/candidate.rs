use std::collections::{BTreeMap, BTreeSet};

use eutheto_planning_ir::{CandidateValues, IntVariableId, ObjectiveLevelId};
use eutheto_protocol::wire::ProjectedCandidate;
use thiserror::Error;

use crate::{CandidateProjectionVariable, TranslatedCpSatModel};

/// Typed candidate values and exact objective levels reconstructed from worker projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedCandidate {
    pub values: CandidateValues,
    pub objective_values: Vec<i64>,
}

/// Safe failures while decoding untrusted projected candidate values.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum CandidateDecodeError {
    #[error("worker candidate repeats projection ID {0}")]
    DuplicateProjection(u64),
    #[error("worker candidate contains unrequested projection ID {0}")]
    UnrequestedProjection(u64),
    #[error("worker candidate omits requested projection ID {0}")]
    MissingProjection(u64),
    #[error("worker candidate contains a non-Boolean value for projection ID {projection_id}")]
    InvalidBooleanValue { projection_id: u64 },
    #[error("worker candidate integer lies outside its translated domain")]
    IntegerOutOfDomain(IntVariableId),
    #[error("translated candidate projection map has no integer domain")]
    MissingIntegerDomain(IntVariableId),
    #[error("worker candidate omits an integer required for exact objective reconstruction")]
    MissingObjectiveVariable(IntVariableId),
    #[error("exact objective reconstruction overflowed")]
    ObjectiveArithmeticOverflow(ObjectiveLevelId),
    #[error("reconstructed objective lies outside its declared exact bounds")]
    ObjectiveOutOfBounds(ObjectiveLevelId),
}

/// Decodes one protocol-validated or raw worker projection into typed local candidate evidence.
///
/// The decoder repeats set validation so callers cannot accidentally bypass protocol-state checks.
/// Boolean values must be exactly zero or one, integers must remain in their translated planning
/// domains, and objective levels are recomputed with checked integer arithmetic rather than trusted
/// from the worker's floating-point summary.
///
/// # Errors
///
/// Returns a typed adapter defect for duplicate, unknown, missing, ill-typed, out-of-domain, or
/// arithmetically inconsistent worker evidence.
pub fn decode_projected_candidate(
    translated: &TranslatedCpSatModel,
    candidate: &ProjectedCandidate,
) -> Result<DecodedCandidate, CandidateDecodeError> {
    let observed = collect_projected_values(candidate)?;
    if let Some(unrequested) = observed
        .keys()
        .find(|projection_id| {
            translated
                .candidate_projection_variable(**projection_id)
                .is_none()
        })
        .copied()
    {
        return Err(CandidateDecodeError::UnrequestedProjection(unrequested));
    }
    if let Some(missing) = translated
        .worker_projection_requests()
        .iter()
        .map(|request| request.projection_id)
        .find(|projection_id| !observed.contains_key(projection_id))
    {
        return Err(CandidateDecodeError::MissingProjection(missing));
    }

    let mut values = CandidateValues::default();
    for (projection_id, value) in observed {
        match translated.candidate_projection_variable(projection_id) {
            Some(CandidateProjectionVariable::Boolean(id)) => {
                let boolean = match value {
                    0 => false,
                    1 => true,
                    _ => {
                        return Err(CandidateDecodeError::InvalidBooleanValue { projection_id });
                    }
                };
                values.booleans.insert(id.clone(), boolean);
            }
            Some(CandidateProjectionVariable::Integer(id)) => {
                let domain = translated
                    .integer_domain(id)
                    .ok_or_else(|| CandidateDecodeError::MissingIntegerDomain(id.clone()))?;
                if !domain.contains(value) {
                    return Err(CandidateDecodeError::IntegerOutOfDomain(id.clone()));
                }
                values.integers.insert(id.clone(), value);
            }
            None => unreachable!("unrequested projections were rejected above"),
        }
    }

    let objective_values = evaluate_objectives(translated, &values)?;
    Ok(DecodedCandidate {
        values,
        objective_values,
    })
}

fn collect_projected_values(
    candidate: &ProjectedCandidate,
) -> Result<BTreeMap<u64, i64>, CandidateDecodeError> {
    let mut values = BTreeMap::new();
    let mut duplicates = BTreeSet::new();
    for projected in &candidate.values {
        if values
            .insert(projected.projection_id, projected.value)
            .is_some()
        {
            duplicates.insert(projected.projection_id);
        }
    }
    if let Some(duplicate) = duplicates.into_iter().next() {
        Err(CandidateDecodeError::DuplicateProjection(duplicate))
    } else {
        Ok(values)
    }
}

fn evaluate_objectives(
    translated: &TranslatedCpSatModel,
    values: &CandidateValues,
) -> Result<Vec<i64>, CandidateDecodeError> {
    let mut objective_values = Vec::with_capacity(translated.objective_plan().levels.len());
    for level in &translated.objective_plan().levels {
        let mut level_value = 0_i64;
        for term in &level.terms {
            let mut term_value = term.expression.constant;
            for expression_term in &term.expression.terms {
                let value = values
                    .integers
                    .get(&expression_term.variable)
                    .ok_or_else(|| {
                        CandidateDecodeError::MissingObjectiveVariable(
                            expression_term.variable.clone(),
                        )
                    })?;
                let contribution =
                    expression_term
                        .coefficient
                        .checked_mul(*value)
                        .ok_or_else(|| {
                            CandidateDecodeError::ObjectiveArithmeticOverflow(level.id.clone())
                        })?;
                term_value = term_value.checked_add(contribution).ok_or_else(|| {
                    CandidateDecodeError::ObjectiveArithmeticOverflow(level.id.clone())
                })?;
            }
            level_value = level_value.checked_add(term_value).ok_or_else(|| {
                CandidateDecodeError::ObjectiveArithmeticOverflow(level.id.clone())
            })?;
        }
        if !(level.lower_bound..=level.upper_bound).contains(&level_value) {
            return Err(CandidateDecodeError::ObjectiveOutOfBounds(level.id.clone()));
        }
        objective_values.push(level_value);
    }
    Ok(objective_values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eutheto_domain_ir::{
        DomainAssignmentId, DomainEntityId, DomainEntityKindId, DomainEntityRef,
        OptimizationDirection, ScoreCategoryId,
    };
    use eutheto_planning_ir::{
        BoolVariable, BoolVariableId, Capability, CompilerId, InclusiveRange, IntDomain,
        IntVariable, LinearExpression, LinearTerm, ObjectiveLevel, ObjectivePlan, ObjectiveTerm,
        ObjectiveTermId, ObjectiveTermKind, PLANNING_IR_SCHEMA_VERSION, PROJECTION_SCHEMA_VERSION,
        PlanningIrLimitsV1, PlanningMetadata, PlanningProblem, ProjectionExpression, ProjectionId,
        ProvenanceId, ProvenanceRecord, ProvenanceSourceKind, SolutionProjection, Variable,
    };
    use eutheto_protocol::wire::ProjectedValue;
    use eutheto_types::{PackId, ScenarioId};
    use std::error::Error;

    fn candidate_problem() -> Result<PlanningProblem, Box<dyn Error>> {
        let provenance = ProvenanceId::new("candidate.provenance")?;
        let bool_id = BoolVariableId::new("candidate.a_bool")?;
        let x = IntVariableId::new("candidate.b_x")?;
        let y = IntVariableId::new("candidate.c_y")?;
        let domain = IntDomain::new(vec![InclusiveRange { start: 0, end: 10 }])?;
        let variables = vec![
            Variable::Boolean(BoolVariable {
                id: bool_id.clone(),
                provenance: provenance.clone(),
            }),
            Variable::Integer(IntVariable {
                id: x.clone(),
                domain: domain.clone(),
                provenance: provenance.clone(),
            }),
            Variable::Integer(IntVariable {
                id: y.clone(),
                domain,
                provenance: provenance.clone(),
            }),
        ];
        let entity_kind = DomainEntityKindId::new("candidate.entity")?;
        let projections = vec![
            projection(
                "boolean",
                entity_kind.clone(),
                ProjectionExpression::Boolean(bool_id),
                &provenance,
            )?,
            projection(
                "linear",
                entity_kind,
                ProjectionExpression::Linear(LinearExpression::new(
                    vec![
                        LinearTerm {
                            variable: x.clone(),
                            coefficient: 1,
                        },
                        LinearTerm {
                            variable: y.clone(),
                            coefficient: 1,
                        },
                    ],
                    0,
                )?),
                &provenance,
            )?,
        ];
        Ok(PlanningProblem {
            schema_version: PLANNING_IR_SCHEMA_VERSION,
            variables,
            constraints: Vec::new(),
            objectives: ObjectivePlan {
                levels: vec![ObjectiveLevel {
                    id: ObjectiveLevelId::new("candidate.objective.level")?,
                    direction: OptimizationDirection::Minimize,
                    lower_bound: 1,
                    upper_bound: 31,
                    terms: vec![ObjectiveTerm {
                        id: ObjectiveTermId::new("candidate.objective.term")?,
                        expression: LinearExpression::new(
                            vec![
                                LinearTerm {
                                    variable: x,
                                    coefficient: 1,
                                },
                                LinearTerm {
                                    variable: y,
                                    coefficient: 2,
                                },
                            ],
                            1,
                        )?,
                        kind: ObjectiveTermKind::Penalty,
                        category: ScoreCategoryId::new("candidate.score")?,
                        provenance: provenance.clone(),
                    }],
                    provenance: provenance.clone(),
                }],
            },
            assumptions: Vec::new(),
            projections,
            provenance: vec![ProvenanceRecord {
                id: provenance,
                source_kind: ProvenanceSourceKind::Fact,
                source_id: "candidate.fixture".to_owned(),
                entity_refs: Vec::new(),
                message_key: "candidate.fixture".to_owned(),
                parameters: BTreeMap::new(),
                parent: None,
            }],
            metadata: fixture_metadata()?,
            declared_capabilities: BTreeSet::from([
                Capability::BooleanProjection,
                Capability::IntegerProjection,
                Capability::ObjectivePenalty,
            ]),
            split_authorization: None,
        })
    }

    fn fixture_metadata() -> Result<PlanningMetadata, Box<dyn Error>> {
        Ok(PlanningMetadata {
            pack_id: PackId::new("official.synthetic")?,
            scenario_id: "01890a5d-ac96-7b64-9f74-bbfcf30f9f80".parse::<ScenarioId>()?,
            scenario_revision: 1,
            projection_version: PROJECTION_SCHEMA_VERSION,
            compiler_id: CompilerId::new("compiler.candidate")?,
            compiler_version: "1.0.0".to_owned(),
            compile_metadata: BTreeMap::new(),
            display_text: BTreeMap::new(),
        })
    }

    fn projection(
        suffix: &str,
        kind: DomainEntityKindId,
        expression: ProjectionExpression,
        provenance: &ProvenanceId,
    ) -> Result<SolutionProjection, Box<dyn Error>> {
        Ok(SolutionProjection {
            id: ProjectionId::new(format!("candidate.projection.{suffix}"))?,
            assignment_id: DomainAssignmentId::new(format!("candidate.assignment.{suffix}"))?,
            entity: DomainEntityRef {
                kind,
                id: DomainEntityId::new(format!("candidate.entity.{suffix}"))?,
            },
            required: true,
            expression,
            provenance: provenance.clone(),
        })
    }

    fn projected(values: &[(u64, i64)]) -> ProjectedCandidate {
        ProjectedCandidate {
            values: values
                .iter()
                .map(|(projection_id, value)| ProjectedValue {
                    projection_id: *projection_id,
                    value: *value,
                })
                .collect(),
        }
    }

    #[test]
    fn decodes_typed_values_and_recomputes_exact_objectives() -> Result<(), Box<dyn Error>> {
        let translated =
            crate::translate_supported_model(&candidate_problem()?, PlanningIrLimitsV1::DEFAULT)?;
        let decoded =
            decode_projected_candidate(&translated, &projected(&[(2, 3), (0, 1), (1, 4)]))?;

        assert_eq!(
            decoded
                .values
                .booleans
                .get(&BoolVariableId::new("candidate.a_bool")?),
            Some(&true)
        );
        assert_eq!(
            decoded
                .values
                .integers
                .get(&IntVariableId::new("candidate.b_x")?),
            Some(&4)
        );
        assert_eq!(
            decoded
                .values
                .integers
                .get(&IntVariableId::new("candidate.c_y")?),
            Some(&3)
        );
        assert_eq!(decoded.objective_values, vec![11]);
        Ok(())
    }

    #[test]
    fn rejects_invalid_projection_sets_deterministically() -> Result<(), Box<dyn Error>> {
        let translated =
            crate::translate_supported_model(&candidate_problem()?, PlanningIrLimitsV1::DEFAULT)?;

        assert_eq!(
            decode_projected_candidate(&translated, &projected(&[(0, 1), (0, 0), (1, 4), (2, 3)])),
            Err(CandidateDecodeError::DuplicateProjection(0))
        );
        assert_eq!(
            decode_projected_candidate(&translated, &projected(&[(0, 1), (1, 4), (2, 3), (99, 7)])),
            Err(CandidateDecodeError::UnrequestedProjection(99))
        );
        assert_eq!(
            decode_projected_candidate(&translated, &projected(&[(0, 1), (1, 4)])),
            Err(CandidateDecodeError::MissingProjection(2))
        );
        Ok(())
    }

    #[test]
    fn rejects_invalid_boolean_and_out_of_domain_integer_values() -> Result<(), Box<dyn Error>> {
        let translated =
            crate::translate_supported_model(&candidate_problem()?, PlanningIrLimitsV1::DEFAULT)?;

        assert_eq!(
            decode_projected_candidate(&translated, &projected(&[(0, 2), (1, 4), (2, 3)])),
            Err(CandidateDecodeError::InvalidBooleanValue { projection_id: 0 })
        );
        assert_eq!(
            decode_projected_candidate(&translated, &projected(&[(0, 1), (1, 11), (2, 3)])),
            Err(CandidateDecodeError::IntegerOutOfDomain(
                IntVariableId::new("candidate.b_x")?
            ))
        );
        Ok(())
    }
}
