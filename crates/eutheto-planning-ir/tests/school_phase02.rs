//! Synthetic foundation gate only: this is not a school domain pack.

use eutheto_domain_ir::{
    DomainAssignmentId, DomainEntityId, DomainEntityKindId, DomainEntityRef, OptimizationDirection,
    ScoreCategoryId,
};
use eutheto_planning_ir::*;
use eutheto_types::{PackId, ScenarioId, SolutionId};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

const SCENARIO_UUID: &str = "01890a5d-ac96-7b64-9f74-bbfcf30f9f80";
const SOLUTION_UUID: &str = "01890a5d-ac96-7b64-9f74-bbfcf30f9f81";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Fixture {
    schema_version: u32,
    scenario_id: String,
    section: Section,
    occurrences: Vec<Occurrence>,
    rooms: Vec<Room>,
    periods: Vec<i64>,
    patterns: Vec<Pattern>,
    linked_rule: LinkedRule,
    #[serde(default)]
    projection_ids: Vec<String>,
    #[serde(default)]
    provenance_ids: Vec<String>,
    #[serde(default)]
    required_capabilities: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Section {
    id: String,
    teacher_id: String,
    cohort_id: String,
    size: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Occurrence {
    id: String,
    kind: String,
    required_count: u32,
    required_equipment: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Room {
    id: String,
    capacity: u32,
    equipment: BTreeSet<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Pattern {
    id: String,
    lecture_period: i64,
    lab_period: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LinkedRule {
    id: String,
    minimum_separation: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Formulation {
    PatternChoice,
    OccurrenceVariables,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ProjectedPlacement {
    section_id: String,
    occurrence_id: String,
    pattern_id: String,
    period: i64,
    room_id: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ProjectedSchedule {
    placements: Vec<ProjectedPlacement>,
    score: i64,
}

#[derive(Clone, Debug, Default)]
struct RawCandidate {
    meeting_selections: BTreeMap<String, Vec<(i64, String)>>,
    room_claims: BTreeSet<(String, i64, String)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateFailure {
    InvalidSchema,
    InvalidRequiredCount,
    MissingOccurrence,
    MultipleMeetings,
    UnknownPeriod,
    UnknownRoom,
    Capacity,
    Equipment,
    FloatingOrMissingRoomLink,
    NoOverlap,
    OrderSeparation,
    UnknownPattern,
}

struct SchoolEncoding {
    problem: PlanningProblem,
    period_variables: BTreeMap<String, IntVariableId>,
    room_variables: BTreeMap<String, IntVariableId>,
    duration_variables: BTreeMap<String, IntVariableId>,
    end_variables: BTreeMap<String, IntVariableId>,
    active_variables: BTreeMap<String, BoolVariableId>,
    room_claim_variables: BTreeMap<(String, usize), BoolVariableId>,
    meeting_variables: BTreeMap<(String, i64), BoolVariableId>,
    assignment_variables: BTreeMap<(String, i64, usize), BoolVariableId>,
    pattern_variables: BTreeMap<String, BoolVariableId>,
}

fn parse_fixture(bytes: &[u8]) -> Result<Fixture, serde_json::Error> {
    serde_json::from_slice(bytes)
}

fn valid_fixture() -> Result<Fixture, serde_json::Error> {
    parse_fixture(include_bytes!(
        "../../../domains/school/fixtures/phase02/mini-school-v1.json"
    ))
}

fn eligible_rooms<'a>(fixture: &'a Fixture, occurrence: &Occurrence) -> Vec<&'a Room> {
    fixture
        .rooms
        .iter()
        .filter(|room| {
            room.capacity >= fixture.section.size
                && room.equipment.contains(&occurrence.required_equipment)
        })
        .collect()
}

fn bool_id(value: impl Into<String>) -> Result<BoolVariableId, PlanningIdError> {
    BoolVariableId::new(value)
}

fn int_id(value: impl Into<String>) -> Result<IntVariableId, PlanningIdError> {
    IntVariableId::new(value)
}

fn interval_id(value: impl Into<String>) -> Result<IntervalVariableId, PlanningIdError> {
    IntervalVariableId::new(value)
}

fn exact_domain(values: impl IntoIterator<Item = i64>) -> Result<IntDomain, ModelError> {
    IntDomain::new(
        values
            .into_iter()
            .map(|value| InclusiveRange {
                start: value,
                end: value,
            })
            .collect(),
    )
}

fn record(
    id: impl Into<String>,
    body: Constraint,
    enforcement: Vec<Literal>,
    provenance: &ProvenanceId,
) -> Result<ConstraintRecord, PlanningIdError> {
    Ok(ConstraintRecord {
        id: PlanningConstraintId::new(id)?,
        body,
        enforcement,
        provenance: provenance.clone(),
        tags: Vec::new(),
    })
}

fn entity(kind: &str, id: &str) -> Result<DomainEntityRef, Box<dyn Error>> {
    Ok(DomainEntityRef {
        kind: DomainEntityKindId::new(kind)?,
        id: DomainEntityId::new(id)?,
    })
}

fn provenance_records(fixture: &Fixture) -> Result<Vec<ProvenanceRecord>, Box<dyn Error>> {
    if fixture.provenance_ids.len() != 5 {
        return Err("school fixture must declare five provenance identities".into());
    }
    let section = ProvenanceId::new(fixture.provenance_ids[0].clone())?;
    let mut records = vec![ProvenanceRecord {
        id: section.clone(),
        source_kind: ProvenanceSourceKind::Fact,
        source_id: fixture.section.id.clone(),
        entity_refs: vec![entity("school.section", &fixture.section.id)?],
        message_key: "school.section.fact".to_owned(),
        parameters: BTreeMap::new(),
        parent: None,
    }];
    for (index, occurrence) in fixture.occurrences.iter().enumerate() {
        let Some(source_id) = fixture.provenance_ids.get(index + 1) else {
            return Err("missing occurrence provenance".into());
        };
        records.push(ProvenanceRecord {
            id: ProvenanceId::new(source_id.clone())?,
            source_kind: ProvenanceSourceKind::Fact,
            source_id: occurrence.id.clone(),
            entity_refs: vec![entity("school.occurrence", &occurrence.id)?],
            message_key: "school.occurrence.fact".to_owned(),
            parameters: BTreeMap::new(),
            parent: Some(section.clone()),
        });
    }
    records.push(ProvenanceRecord {
        id: ProvenanceId::new(fixture.provenance_ids[3].clone())?,
        source_kind: ProvenanceSourceKind::Derived,
        source_id: "school.room-link".to_owned(),
        entity_refs: vec![entity("school.section", &fixture.section.id)?],
        message_key: "school.room-link".to_owned(),
        parameters: BTreeMap::new(),
        parent: Some(section.clone()),
    });
    records.push(ProvenanceRecord {
        id: ProvenanceId::new(fixture.provenance_ids[4].clone())?,
        source_kind: ProvenanceSourceKind::RequiredRule,
        source_id: fixture.linked_rule.id.clone(),
        entity_refs: vec![entity("school.section", &fixture.section.id)?],
        message_key: "school.order".to_owned(),
        parameters: BTreeMap::new(),
        parent: Some(section),
    });
    Ok(records)
}

// This exhaustive fixture constructor intentionally keeps every emitted variable and
// constraint together so the two formulations remain directly auditable.
#[allow(clippy::too_many_lines)]
fn build_problem(
    fixture: &Fixture,
    formulation: Formulation,
    require_preferred_early_lecture: bool,
) -> Result<SchoolEncoding, Box<dyn Error>> {
    if fixture.schema_version != 1 || fixture.occurrences.len() != 2 || fixture.periods.is_empty() {
        return Err("unsupported bounded school fixture shape".into());
    }
    let lecture = fixture
        .occurrences
        .iter()
        .find(|occurrence| occurrence.kind == "lecture")
        .ok_or("missing lecture occurrence")?;
    let lab = fixture
        .occurrences
        .iter()
        .find(|occurrence| occurrence.kind == "lab")
        .ok_or("missing lab occurrence")?;
    let provenance = provenance_records(fixture)?;
    let section_provenance = ProvenanceId::new(fixture.provenance_ids[0].clone())?;
    let room_link_provenance = ProvenanceId::new(fixture.provenance_ids[3].clone())?;
    let order_provenance = ProvenanceId::new(fixture.provenance_ids[4].clone())?;
    let mut variables = Vec::new();
    let mut constraints = Vec::new();
    let mut period_variables = BTreeMap::new();
    let mut room_variables = BTreeMap::new();
    let mut duration_variables = BTreeMap::new();
    let mut end_variables = BTreeMap::new();
    let mut active_variables = BTreeMap::new();
    let mut room_claim_variables = BTreeMap::new();
    let mut meeting_variables = BTreeMap::new();
    let mut assignment_variables = BTreeMap::new();
    let mut intervals = Vec::new();
    let maximum_period = *fixture.periods.iter().max().ok_or("empty periods")?;

    for (occurrence_index, occurrence) in fixture.occurrences.iter().enumerate() {
        let occurrence_provenance = ProvenanceId::new(
            fixture
                .provenance_ids
                .get(occurrence_index + 1)
                .ok_or("missing occurrence provenance")?
                .clone(),
        )?;
        let period = int_id(format!("school.var.period.{}", occurrence.id))?;
        let room = int_id(format!("school.var.room.{}", occurrence.id))?;
        variables.extend([
            Variable::Integer(IntVariable {
                id: period.clone(),
                domain: exact_domain(fixture.periods.iter().copied())?,
                provenance: occurrence_provenance.clone(),
            }),
            Variable::Integer(IntVariable {
                id: room.clone(),
                domain: exact_domain(0..i64::try_from(fixture.rooms.len())?)?,
                provenance: occurrence_provenance.clone(),
            }),
        ]);

        if formulation == Formulation::PatternChoice {
            let duration = int_id(format!("school.var.duration.{}", occurrence.id))?;
            let end = int_id(format!("school.var.end.{}", occurrence.id))?;
            let active = bool_id(format!("school.var.active.{}", occurrence.id))?;
            let interval = interval_id(format!("school.var.interval.{}", occurrence.id))?;
            variables.extend([
                Variable::Integer(IntVariable {
                    id: duration.clone(),
                    domain: exact_domain([1])?,
                    provenance: occurrence_provenance.clone(),
                }),
                Variable::Integer(IntVariable {
                    id: end.clone(),
                    domain: exact_domain(
                        1..=maximum_period.checked_add(1).ok_or("period overflow")?,
                    )?,
                    provenance: occurrence_provenance.clone(),
                }),
                Variable::Boolean(BoolVariable {
                    id: active.clone(),
                    provenance: occurrence_provenance.clone(),
                }),
                Variable::Interval(IntervalVariable {
                    id: interval.clone(),
                    start: period.clone(),
                    duration: duration.clone(),
                    end: end.clone(),
                    presence: None,
                    provenance: occurrence_provenance.clone(),
                }),
            ]);
            constraints.push(record(
                format!("school.constraint.required-count.{}", occurrence.id),
                Constraint::exactly_one(if occurrence.required_count == 1 {
                    vec![Literal::positive(active.clone())]
                } else {
                    Vec::new()
                }),
                Vec::new(),
                &room_link_provenance,
            )?);
            let mut room_literals = Vec::new();
            let mut eligible_rows = Vec::new();
            for (room_index, candidate_room) in fixture.rooms.iter().enumerate() {
                let claim = bool_id(format!(
                    "school.var.room-claim.{}.{}",
                    occurrence.id, candidate_room.id
                ))?;
                variables.push(Variable::Boolean(BoolVariable {
                    id: claim.clone(),
                    provenance: room_link_provenance.clone(),
                }));
                room_literals.push(Literal::positive(claim.clone()));
                room_claim_variables.insert((occurrence.id.clone(), room_index), claim.clone());
                constraints.push(record(
                    format!(
                        "school.constraint.room-link.{}.{}",
                        occurrence.id, candidate_room.id
                    ),
                    Constraint::LinearComparison(LinearComparison {
                        expression: LinearExpression::new(
                            vec![LinearTerm {
                                variable: room.clone(),
                                coefficient: 1,
                            }],
                            0,
                        )?,
                        op: ComparisonOp::Equal,
                        rhs: i64::try_from(room_index)?,
                    }),
                    vec![Literal::positive(claim)],
                    &room_link_provenance,
                )?);
                if candidate_room.capacity >= fixture.section.size
                    && candidate_room
                        .equipment
                        .contains(&occurrence.required_equipment)
                {
                    eligible_rows.push(vec![i64::try_from(room_index)?]);
                }
            }
            constraints.push(record(
                format!("school.constraint.one-room.{}", occurrence.id),
                Constraint::exactly_one(room_literals),
                Vec::new(),
                &room_link_provenance,
            )?);
            constraints.push(record(
                format!("school.constraint.eligible-room.{}", occurrence.id),
                Constraint::allowed_table(vec![room.clone()], eligible_rows)?,
                Vec::new(),
                &room_link_provenance,
            )?);
            duration_variables.insert(occurrence.id.clone(), duration);
            end_variables.insert(occurrence.id.clone(), end);
            active_variables.insert(occurrence.id.clone(), active);
            intervals.push(interval);
        } else {
            let eligible_room_indices = fixture
                .rooms
                .iter()
                .enumerate()
                .filter_map(|(room_index, candidate_room)| {
                    (candidate_room.capacity >= fixture.section.size
                        && candidate_room
                            .equipment
                            .contains(&occurrence.required_equipment))
                    .then_some(room_index)
                })
                .collect::<Vec<_>>();
            constraints.push(record(
                format!("school.constraint.capacity-equipment.{}", occurrence.id),
                Constraint::allowed_table(
                    vec![room.clone()],
                    eligible_room_indices
                        .iter()
                        .map(|room_index| Ok(vec![i64::try_from(*room_index)?]))
                        .collect::<Result<Vec<_>, Box<dyn Error>>>()?,
                )?,
                Vec::new(),
                &room_link_provenance,
            )?);
            let mut room_literals = Vec::new();
            for &room_index in &eligible_room_indices {
                let claim = bool_id(format!(
                    "school.var.room-claim.{}.{}",
                    occurrence.id, fixture.rooms[room_index].id
                ))?;
                variables.push(Variable::Boolean(BoolVariable {
                    id: claim.clone(),
                    provenance: room_link_provenance.clone(),
                }));
                room_literals.push(Literal::positive(claim.clone()));
                room_claim_variables.insert((occurrence.id.clone(), room_index), claim);
            }

            let mut assignment_literals = Vec::new();
            let mut meeting_literals = Vec::new();
            for &allowed_period in &fixture.periods {
                let meeting = bool_id(format!(
                    "school.var.meeting.{}.{}",
                    occurrence.id, allowed_period
                ))?;
                variables.push(Variable::Boolean(BoolVariable {
                    id: meeting.clone(),
                    provenance: occurrence_provenance.clone(),
                }));
                meeting_literals.push(Literal::positive(meeting.clone()));
                meeting_variables.insert((occurrence.id.clone(), allowed_period), meeting.clone());
                let mut assignments_at_period = Vec::new();
                for &room_index in &eligible_room_indices {
                    let assignment = bool_id(format!(
                        "school.var.assignment.{}.{}.{}",
                        occurrence.id, allowed_period, fixture.rooms[room_index].id
                    ))?;
                    variables.push(Variable::Boolean(BoolVariable {
                        id: assignment.clone(),
                        provenance: room_link_provenance.clone(),
                    }));
                    assignment_literals.push(Literal::positive(assignment.clone()));
                    assignments_at_period.push(Literal::positive(assignment.clone()));
                    assignment_variables.insert(
                        (occurrence.id.clone(), allowed_period, room_index),
                        assignment.clone(),
                    );
                    constraints.push(record(
                        format!(
                            "school.constraint.assignment-meeting.{}.{}.{}",
                            occurrence.id, allowed_period, room_index
                        ),
                        Constraint::Implication {
                            antecedent: Literal::positive(assignment.clone()),
                            consequent: Literal::positive(meeting.clone()),
                        },
                        Vec::new(),
                        &room_link_provenance,
                    )?);
                    let claim = room_claim_variables
                        .get(&(occurrence.id.clone(), room_index))
                        .ok_or("missing eligible room claim")?
                        .clone();
                    constraints.push(record(
                        format!(
                            "school.constraint.assignment-room-claim.{}.{}.{}",
                            occurrence.id, allowed_period, room_index
                        ),
                        Constraint::Implication {
                            antecedent: Literal::positive(assignment.clone()),
                            consequent: Literal::positive(claim),
                        },
                        Vec::new(),
                        &room_link_provenance,
                    )?);
                    for (component, variable, rhs) in [
                        ("period", period.clone(), allowed_period),
                        ("room", room.clone(), i64::try_from(room_index)?),
                    ] {
                        constraints.push(record(
                            format!(
                                "school.constraint.assignment-{}.{}.{}.{}",
                                component, occurrence.id, allowed_period, room_index
                            ),
                            Constraint::LinearComparison(LinearComparison {
                                expression: LinearExpression::new(
                                    vec![LinearTerm {
                                        variable,
                                        coefficient: 1,
                                    }],
                                    0,
                                )?,
                                op: ComparisonOp::Equal,
                                rhs,
                            }),
                            vec![Literal::positive(assignment.clone())],
                            &room_link_provenance,
                        )?);
                    }
                }
                constraints.push(record(
                    format!(
                        "school.constraint.meeting-to-assignment.{}.{}",
                        occurrence.id, allowed_period
                    ),
                    Constraint::bool_or(assignments_at_period),
                    vec![Literal::positive(meeting)],
                    &room_link_provenance,
                )?);
            }
            for &room_index in &eligible_room_indices {
                let claim = room_claim_variables
                    .get(&(occurrence.id.clone(), room_index))
                    .ok_or("missing room claim")?
                    .clone();
                let assignments_for_room = fixture
                    .periods
                    .iter()
                    .map(|allowed_period| {
                        assignment_variables
                            .get(&(occurrence.id.clone(), *allowed_period, room_index))
                            .cloned()
                            .map(Literal::positive)
                            .ok_or("missing occurrence assignment")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                constraints.push(record(
                    format!(
                        "school.constraint.room-to-assignment.{}.{}",
                        occurrence.id, room_index
                    ),
                    Constraint::bool_or(assignments_for_room),
                    vec![Literal::positive(claim)],
                    &room_link_provenance,
                )?);
            }
            if occurrence.required_count != 1 {
                assignment_literals.clear();
            }
            constraints.push(record(
                format!("school.constraint.required-count.{}", occurrence.id),
                Constraint::exactly_one(assignment_literals),
                Vec::new(),
                &room_link_provenance,
            )?);
            constraints.push(record(
                format!("school.constraint.one-meeting.{}", occurrence.id),
                Constraint::exactly_one(meeting_literals),
                Vec::new(),
                &room_link_provenance,
            )?);
            constraints.push(record(
                format!("school.constraint.one-room.{}", occurrence.id),
                Constraint::exactly_one(room_literals),
                Vec::new(),
                &room_link_provenance,
            )?);
        }
        period_variables.insert(occurrence.id.clone(), period);
        room_variables.insert(occurrence.id.clone(), room);
    }

    let lecture_period = period_variables
        .get(&lecture.id)
        .ok_or("missing lecture period variable")?
        .clone();
    let lab_period = period_variables
        .get(&lab.id)
        .ok_or("missing lab period variable")?
        .clone();
    constraints.push(record(
        "school.constraint.pattern-table",
        Constraint::allowed_table(
            vec![lecture_period.clone(), lab_period.clone()],
            fixture
                .patterns
                .iter()
                .map(|pattern| vec![pattern.lecture_period, pattern.lab_period])
                .collect(),
        )?,
        Vec::new(),
        &order_provenance,
    )?);
    if formulation == Formulation::PatternChoice {
        constraints.push(record(
            "school.constraint.no-overlap",
            Constraint::no_overlap(intervals),
            Vec::new(),
            &order_provenance,
        )?);
    } else {
        for &allowed_period in &fixture.periods {
            let simultaneous_meetings = fixture
                .occurrences
                .iter()
                .map(|occurrence| {
                    meeting_variables
                        .get(&(occurrence.id.clone(), allowed_period))
                        .cloned()
                        .map(Literal::positive)
                        .ok_or("missing meeting variable")
                })
                .collect::<Result<Vec<_>, _>>()?;
            constraints.push(record(
                format!("school.constraint.teacher-cohort-no-overlap.{allowed_period}"),
                Constraint::at_most_one(simultaneous_meetings),
                Vec::new(),
                &order_provenance,
            )?);
            for room_index in 0..fixture.rooms.len() {
                let simultaneous_room_assignments = fixture
                    .occurrences
                    .iter()
                    .filter_map(|occurrence| {
                        assignment_variables
                            .get(&(occurrence.id.clone(), allowed_period, room_index))
                            .cloned()
                            .map(Literal::positive)
                    })
                    .collect::<Vec<_>>();
                if simultaneous_room_assignments.len() > 1 {
                    constraints.push(record(
                        format!("school.constraint.room-capacity.{allowed_period}.{room_index}"),
                        Constraint::at_most_one(simultaneous_room_assignments),
                        Vec::new(),
                        &room_link_provenance,
                    )?);
                }
            }
        }
    }
    constraints.push(record(
        "school.constraint.order-separation",
        Constraint::LinearComparison(LinearComparison {
            expression: LinearExpression::new(
                vec![
                    LinearTerm {
                        variable: lab_period.clone(),
                        coefficient: 1,
                    },
                    LinearTerm {
                        variable: lecture_period.clone(),
                        coefficient: -1,
                    },
                ],
                0,
            )?,
            op: ComparisonOp::GreaterOrEqual,
            rhs: fixture.linked_rule.minimum_separation,
        }),
        Vec::new(),
        &order_provenance,
    )?);

    let mut pattern_variables = BTreeMap::new();
    if formulation == Formulation::PatternChoice {
        let mut literals = Vec::new();
        for pattern in &fixture.patterns {
            let selected = bool_id(format!("school.var.pattern.{}", pattern.id))?;
            variables.push(Variable::Boolean(BoolVariable {
                id: selected.clone(),
                provenance: order_provenance.clone(),
            }));
            literals.push(Literal::positive(selected.clone()));
            for (suffix, variable, rhs) in [
                ("lecture", lecture_period.clone(), pattern.lecture_period),
                ("lab", lab_period.clone(), pattern.lab_period),
            ] {
                constraints.push(record(
                    format!("school.constraint.pattern.{}.{}", pattern.id, suffix),
                    Constraint::LinearComparison(LinearComparison {
                        expression: LinearExpression::new(
                            vec![LinearTerm {
                                variable,
                                coefficient: 1,
                            }],
                            0,
                        )?,
                        op: ComparisonOp::Equal,
                        rhs,
                    }),
                    vec![Literal::positive(selected.clone())],
                    &order_provenance,
                )?);
            }
            pattern_variables.insert(pattern.id.clone(), selected);
        }
        constraints.push(record(
            "school.constraint.one-pattern",
            Constraint::exactly_one(literals),
            Vec::new(),
            &order_provenance,
        )?);
    }
    if require_preferred_early_lecture {
        constraints.push(record(
            "school.constraint.preference-promoted-to-required",
            Constraint::LinearComparison(LinearComparison {
                expression: LinearExpression::new(
                    vec![LinearTerm {
                        variable: lecture_period.clone(),
                        coefficient: 1,
                    }],
                    0,
                )?,
                op: ComparisonOp::LessOrEqual,
                rhs: *fixture.periods.iter().min().ok_or("empty periods")?,
            }),
            Vec::new(),
            &order_provenance,
        )?);
    }

    let objective_terms = [lecture, lab]
        .into_iter()
        .map(|occurrence| {
            Ok(ObjectiveTerm {
                id: ObjectiveTermId::new(format!("school.objective.{}", occurrence.id))?,
                expression: LinearExpression::new(
                    vec![LinearTerm {
                        variable: period_variables
                            .get(&occurrence.id)
                            .ok_or("missing objective period")?
                            .clone(),
                        coefficient: 1,
                    }],
                    0,
                )?,
                kind: ObjectiveTermKind::Penalty,
                category: ScoreCategoryId::new("school.score.period")?,
                provenance: section_provenance.clone(),
            })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let minimum_period = *fixture.periods.iter().min().ok_or("empty periods")?;
    let maximum_period = *fixture.periods.iter().max().ok_or("empty periods")?;
    let room_count = i64::try_from(fixture.rooms.len())?;
    let projections = fixture
        .occurrences
        .iter()
        .zip(&fixture.projection_ids)
        .map(|(occurrence, projection_id)| {
            Ok(SolutionProjection {
                id: ProjectionId::new(projection_id.clone())?,
                assignment_id: DomainAssignmentId::new(format!(
                    "school.assignment.{}",
                    occurrence.id
                ))?,
                entity: entity("school.occurrence", &occurrence.id)?,
                required: true,
                expression: ProjectionExpression::Linear(LinearExpression::new(
                    vec![
                        LinearTerm {
                            variable: period_variables
                                .get(&occurrence.id)
                                .ok_or("missing projection period")?
                                .clone(),
                            coefficient: room_count,
                        },
                        LinearTerm {
                            variable: room_variables
                                .get(&occurrence.id)
                                .ok_or("missing projection room")?
                                .clone(),
                            coefficient: 1,
                        },
                    ],
                    0,
                )?),
                provenance: room_link_provenance.clone(),
            })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    if projections.len() != fixture.occurrences.len() {
        return Err("every occurrence must have one stable projection".into());
    }
    let mut problem = PlanningProblem {
        schema_version: PLANNING_IR_SCHEMA_VERSION,
        variables,
        constraints,
        objectives: ObjectivePlan {
            levels: vec![ObjectiveLevel {
                id: ObjectiveLevelId::new("school.objective-level.preference")?,
                direction: OptimizationDirection::Minimize,
                lower_bound: minimum_period.checked_mul(2).ok_or("objective overflow")?,
                upper_bound: maximum_period.checked_mul(2).ok_or("objective overflow")?,
                terms: objective_terms,
                provenance: section_provenance,
            }],
        },
        assumptions: Vec::new(),
        projections,
        provenance,
        metadata: PlanningMetadata {
            pack_id: PackId::new("official.school-phase02-fixture")?,
            scenario_id: SCENARIO_UUID.parse::<ScenarioId>()?,
            scenario_revision: 1,
            projection_version: PROJECTION_SCHEMA_VERSION,
            compiler_id: CompilerId::new(match formulation {
                Formulation::PatternChoice => "compiler.school-pattern-choice",
                Formulation::OccurrenceVariables => "compiler.school-occurrence-variables",
            })?,
            compiler_version: "1.0.0".to_owned(),
            compile_metadata: BTreeMap::from([(
                MetadataKey::new("school.scenario-id")?,
                ProvenanceParameter::Text(fixture.scenario_id.clone()),
            )]),
            display_text: BTreeMap::from([("scenarioName".to_owned(), "Mini school".to_owned())]),
        },
        declared_capabilities: BTreeSet::new(),
        split_authorization: None,
    };
    problem.canonicalize()?;
    validate(&problem, PlanningIrLimitsV1::DEFAULT)?;
    let _ = summarize(&problem, PlanningIrLimitsV1::DEFAULT)?;
    let _ = canonical_ir_hash(&problem, PlanningIrLimitsV1::DEFAULT)?;
    Ok(SchoolEncoding {
        problem,
        period_variables,
        room_variables,
        duration_variables,
        end_variables,
        active_variables,
        room_claim_variables,
        meeting_variables,
        assignment_variables,
        pattern_variables,
    })
}

fn literal_value(literal: &Literal, candidate: &CandidateValues) -> Option<bool> {
    candidate
        .booleans
        .get(&literal.variable)
        .map(|value| *value == literal.positive)
}

fn expression_value(expression: &LinearExpression, candidate: &CandidateValues) -> Option<i64> {
    expression
        .terms
        .iter()
        .try_fold(expression.constant, |sum, term| {
            sum.checked_add(
                candidate
                    .integers
                    .get(&term.variable)?
                    .checked_mul(term.coefficient)?,
            )
        })
}

fn comparison_value(comparison: &LinearComparison, candidate: &CandidateValues) -> bool {
    let Some(value) = expression_value(&comparison.expression, candidate) else {
        return false;
    };
    match comparison.op {
        ComparisonOp::Equal => value == comparison.rhs,
        ComparisonOp::LessOrEqual => value <= comparison.rhs,
        ComparisonOp::GreaterOrEqual => value >= comparison.rhs,
    }
}

fn constraint_value(
    constraint: &Constraint,
    problem: &PlanningProblem,
    candidate: &CandidateValues,
) -> bool {
    match constraint {
        Constraint::ExactlyOne { literals } => {
            literals
                .iter()
                .filter(|literal| literal_value(literal, candidate) == Some(true))
                .count()
                == 1
        }
        Constraint::BoolOr { literals } => literals
            .iter()
            .any(|literal| literal_value(literal, candidate) == Some(true)),
        Constraint::Implication {
            antecedent,
            consequent,
        } => {
            literal_value(antecedent, candidate) == Some(false)
                || literal_value(consequent, candidate) == Some(true)
        }
        Constraint::AtMostOne { literals } => {
            literals
                .iter()
                .filter(|literal| literal_value(literal, candidate) == Some(true))
                .count()
                <= 1
        }
        Constraint::LinearComparison(comparison) => comparison_value(comparison, candidate),
        Constraint::AllowedTable { variables, rows } => rows.iter().any(|row| {
            variables
                .iter()
                .zip(row)
                .all(|(variable, expected)| candidate.integers.get(variable) == Some(expected))
        }),
        Constraint::NoOverlap { intervals } => {
            let mut ranges = Vec::new();
            for interval_id in intervals {
                let Some(Variable::Interval(interval)) = problem.variables.iter().find(|variable| {
                    matches!(variable, Variable::Interval(value) if &value.id == interval_id)
                }) else {
                    return false;
                };
                let (Some(start), Some(end)) = (
                    candidate.integers.get(&interval.start),
                    candidate.integers.get(&interval.end),
                ) else {
                    return false;
                };
                ranges.push((*start, *end));
            }
            ranges.iter().enumerate().all(|(index, left)| {
                ranges
                    .iter()
                    .skip(index + 1)
                    .all(|right| left.1 <= right.0 || right.1 <= left.0)
            })
        }
        Constraint::BoolAnd { .. }
        | Constraint::Equivalence { .. }
        | Constraint::CardinalityRange { .. }
        | Constraint::ReifiedLinearComparison { .. }
        | Constraint::AllDifferent { .. }
        | Constraint::ForbiddenTable { .. }
        | Constraint::Element { .. }
        | Constraint::Min { .. }
        | Constraint::Max { .. }
        | Constraint::Equality { .. }
        | Constraint::AbsDifference { .. }
        | Constraint::Cumulative { .. } => false,
    }
}

fn candidate_satisfies(encoding: &SchoolEncoding, candidate: &CandidateValues) -> bool {
    for variable in &encoding.problem.variables {
        match variable {
            Variable::Boolean(value) => {
                if !candidate.booleans.contains_key(&value.id) {
                    return false;
                }
            }
            Variable::Integer(value) => {
                let Some(assigned) = candidate.integers.get(&value.id) else {
                    return false;
                };
                if !value.domain.contains(*assigned) {
                    return false;
                }
            }
            Variable::Interval(value) => {
                let (Some(start), Some(duration), Some(end)) = (
                    candidate.integers.get(&value.start),
                    candidate.integers.get(&value.duration),
                    candidate.integers.get(&value.end),
                ) else {
                    return false;
                };
                if *duration < 0 || start.checked_add(*duration) != Some(*end) {
                    return false;
                }
            }
        }
    }
    encoding.problem.constraints.iter().all(|record| {
        if record
            .enforcement
            .iter()
            .any(|literal| literal_value(literal, candidate) != Some(true))
        {
            true
        } else {
            constraint_value(&record.body, &encoding.problem, candidate)
        }
    })
}

fn assign_occurrence(
    candidate: &mut CandidateValues,
    encoding: &SchoolEncoding,
    occurrence: &Occurrence,
    period: i64,
    room_index: usize,
) -> Result<(), Box<dyn Error>> {
    candidate
        .integers
        .insert(encoding.period_variables[&occurrence.id].clone(), period);
    candidate.integers.insert(
        encoding.room_variables[&occurrence.id].clone(),
        i64::try_from(room_index)?,
    );
    if let Some(duration) = encoding.duration_variables.get(&occurrence.id) {
        candidate.integers.insert(duration.clone(), 1);
    }
    if let Some(end) = encoding.end_variables.get(&occurrence.id) {
        candidate
            .integers
            .insert(end.clone(), period.checked_add(1).ok_or("period overflow")?);
    }
    if let Some(active) = encoding.active_variables.get(&occurrence.id) {
        candidate.booleans.insert(active.clone(), true);
    }
    if let Some(claim) = encoding
        .room_claim_variables
        .get(&(occurrence.id.clone(), room_index))
    {
        candidate.booleans.insert(claim.clone(), true);
    }
    if let Some(meeting) = encoding
        .meeting_variables
        .get(&(occurrence.id.clone(), period))
    {
        candidate.booleans.insert(meeting.clone(), true);
    }
    if let Some(assignment) =
        encoding
            .assignment_variables
            .get(&(occurrence.id.clone(), period, room_index))
    {
        candidate.booleans.insert(assignment.clone(), true);
    }
    Ok(())
}

fn occurrence_candidate(
    encoding: &SchoolEncoding,
    placements: [(&Occurrence, i64, usize); 2],
) -> Result<CandidateValues, Box<dyn Error>> {
    let mut candidate = CandidateValues::default();
    for variable in &encoding.problem.variables {
        if let Variable::Boolean(value) = variable {
            candidate.booleans.insert(value.id.clone(), false);
        }
    }
    for (occurrence, period, room_index) in placements {
        assign_occurrence(&mut candidate, encoding, occurrence, period, room_index)?;
    }
    Ok(candidate)
}

fn enumerate(
    fixture: &Fixture,
    formulation: Formulation,
    require_preferred_early_lecture: bool,
) -> Result<(SchoolEncoding, BTreeSet<ProjectedSchedule>), Box<dyn Error>> {
    let encoding = build_problem(fixture, formulation, require_preferred_early_lecture)?;
    let lecture = fixture
        .occurrences
        .iter()
        .find(|occurrence| occurrence.kind == "lecture")
        .ok_or("missing lecture")?;
    let lab = fixture
        .occurrences
        .iter()
        .find(|occurrence| occurrence.kind == "lab")
        .ok_or("missing lab")?;
    let mut schedules = BTreeSet::new();
    for lecture_period in &fixture.periods {
        for lecture_room in 0..fixture.rooms.len() {
            for lab_period in &fixture.periods {
                for lab_room in 0..fixture.rooms.len() {
                    let mut candidate = CandidateValues::default();
                    for variable in &encoding.problem.variables {
                        if let Variable::Boolean(value) = variable {
                            candidate.booleans.insert(value.id.clone(), false);
                        }
                    }
                    for (occurrence, period, room_index) in [
                        (lecture, *lecture_period, lecture_room),
                        (lab, *lab_period, lab_room),
                    ] {
                        assign_occurrence(
                            &mut candidate,
                            &encoding,
                            occurrence,
                            period,
                            room_index,
                        )?;
                    }
                    if let Some(pattern) = fixture.patterns.iter().find(|pattern| {
                        pattern.lecture_period == *lecture_period
                            && pattern.lab_period == *lab_period
                    }) && let Some(variable) = encoding.pattern_variables.get(&pattern.id)
                    {
                        candidate.booleans.insert(variable.clone(), true);
                    }
                    if !candidate_satisfies(&encoding, &candidate) {
                        continue;
                    }
                    let normalized = project_candidate(
                        &encoding.problem,
                        &candidate,
                        SOLUTION_UUID.parse::<SolutionId>()?,
                        PlanningIrLimitsV1::DEFAULT,
                    )?;
                    if normalized.assignments.len() != fixture.occurrences.len() {
                        return Err("public projection omitted an occurrence".into());
                    }
                    let pattern = fixture
                        .patterns
                        .iter()
                        .find(|pattern| {
                            pattern.lecture_period == *lecture_period
                                && pattern.lab_period == *lab_period
                        })
                        .ok_or("encoded candidate has no pattern")?;
                    let mut placements = vec![
                        ProjectedPlacement {
                            section_id: fixture.section.id.clone(),
                            occurrence_id: lecture.id.clone(),
                            pattern_id: pattern.id.clone(),
                            period: *lecture_period,
                            room_id: fixture.rooms[lecture_room].id.clone(),
                        },
                        ProjectedPlacement {
                            section_id: fixture.section.id.clone(),
                            occurrence_id: lab.id.clone(),
                            pattern_id: pattern.id.clone(),
                            period: *lab_period,
                            room_id: fixture.rooms[lab_room].id.clone(),
                        },
                    ];
                    placements.sort();
                    let score = encoding.problem.objectives.levels[0]
                        .terms
                        .iter()
                        .try_fold(0_i64, |total, term| {
                            total.checked_add(expression_value(&term.expression, &candidate)?)
                        })
                        .ok_or("objective overflow")?;
                    schedules.insert(ProjectedSchedule { placements, score });
                }
            }
        }
    }
    Ok((encoding, schedules))
}

fn linked_candidate(
    lecture: &Occurrence,
    lecture_period: i64,
    lecture_room: &Room,
    lab: &Occurrence,
    lab_period: i64,
    lab_room: &Room,
) -> RawCandidate {
    RawCandidate {
        meeting_selections: BTreeMap::from([
            (
                lecture.id.clone(),
                vec![(lecture_period, lecture_room.id.clone())],
            ),
            (lab.id.clone(), vec![(lab_period, lab_room.id.clone())]),
        ]),
        room_claims: BTreeSet::from([
            (lecture.id.clone(), lecture_period, lecture_room.id.clone()),
            (lab.id.clone(), lab_period, lab_room.id.clone()),
        ]),
    }
}

fn placements_overlap(fixture: &Fixture, placements: &[(&Occurrence, i64, String)]) -> bool {
    let has_shared_resource =
        !fixture.section.teacher_id.is_empty() || !fixture.section.cohort_id.is_empty();
    placements
        .iter()
        .enumerate()
        .any(|(index, (_, period, room))| {
            placements
                .iter()
                .skip(index + 1)
                .any(|(_, other_period, other_room)| {
                    period == other_period && (has_shared_resource || room == other_room)
                })
        })
}

fn validate_candidate(
    fixture: &Fixture,
    candidate: &RawCandidate,
) -> Result<ProjectedSchedule, CandidateFailure> {
    if fixture.schema_version != 1 {
        return Err(CandidateFailure::InvalidSchema);
    }
    if fixture
        .occurrences
        .iter()
        .any(|occurrence| occurrence.required_count != 1)
    {
        return Err(CandidateFailure::InvalidRequiredCount);
    }
    if candidate.meeting_selections.len() != fixture.occurrences.len() {
        return Err(CandidateFailure::MissingOccurrence);
    }
    let mut selected_claims = BTreeSet::new();
    let mut placements = Vec::new();
    for occurrence in &fixture.occurrences {
        let selected = candidate
            .meeting_selections
            .get(&occurrence.id)
            .ok_or(CandidateFailure::MissingOccurrence)?;
        let [(period, room_id)] = selected.as_slice() else {
            return Err(CandidateFailure::MultipleMeetings);
        };
        if !fixture.periods.contains(period) {
            return Err(CandidateFailure::UnknownPeriod);
        }
        let room = fixture
            .rooms
            .iter()
            .find(|room| &room.id == room_id)
            .ok_or(CandidateFailure::UnknownRoom)?;
        if room.capacity < fixture.section.size {
            return Err(CandidateFailure::Capacity);
        }
        if !room.equipment.contains(&occurrence.required_equipment) {
            return Err(CandidateFailure::Equipment);
        }
        selected_claims.insert((occurrence.id.clone(), *period, room_id.clone()));
        placements.push((occurrence, *period, room_id.clone()));
    }
    if selected_claims != candidate.room_claims {
        return Err(CandidateFailure::FloatingOrMissingRoomLink);
    }
    if placements_overlap(fixture, &placements) {
        return Err(CandidateFailure::NoOverlap);
    }
    let lecture = placements
        .iter()
        .find(|(occurrence, _, _)| occurrence.kind == "lecture")
        .ok_or(CandidateFailure::MissingOccurrence)?;
    let lab = placements
        .iter()
        .find(|(occurrence, _, _)| occurrence.kind == "lab")
        .ok_or(CandidateFailure::MissingOccurrence)?;
    if lab
        .1
        .checked_sub(lecture.1)
        .ok_or(CandidateFailure::OrderSeparation)?
        < fixture.linked_rule.minimum_separation
    {
        return Err(CandidateFailure::OrderSeparation);
    }
    let pattern = fixture
        .patterns
        .iter()
        .find(|pattern| pattern.lecture_period == lecture.1 && pattern.lab_period == lab.1)
        .ok_or(CandidateFailure::UnknownPattern)?;
    let mut projected: Vec<_> = placements
        .into_iter()
        .map(|(occurrence, period, room_id)| ProjectedPlacement {
            section_id: fixture.section.id.clone(),
            occurrence_id: occurrence.id.clone(),
            pattern_id: pattern.id.clone(),
            period,
            room_id,
        })
        .collect();
    projected.sort();
    let score = projected
        .iter()
        .try_fold(0_i64, |total, placement| {
            total.checked_add(placement.period)
        })
        .ok_or(CandidateFailure::OrderSeparation)?;
    Ok(ProjectedSchedule {
        placements: projected,
        score,
    })
}

fn capability_name(capability: Capability) -> &'static str {
    match capability {
        Capability::ExactlyOne => "exactlyOne",
        Capability::LinearComparison => "linearComparison",
        Capability::AllowedTable => "allowedTable",
        Capability::NoOverlap => "noOverlap",
        Capability::ObjectivePenalty => "objectivePenalty",
        Capability::IntegerProjection => "integerProjection",
        _ => "unexpected",
    }
}

#[test]
fn exhaustive_public_ir_formulations_have_identical_projection_and_score()
-> Result<(), Box<dyn Error>> {
    let fixture = valid_fixture()?;
    let (pattern_ir, patterns) = enumerate(&fixture, Formulation::PatternChoice, false)?;
    let (occurrence_ir, occurrences) =
        enumerate(&fixture, Formulation::OccurrenceVariables, false)?;
    assert!(!patterns.is_empty());
    assert_eq!(patterns, occurrences);
    assert_eq!(patterns.len(), 8);
    validate(&pattern_ir.problem, PlanningIrLimitsV1::DEFAULT)?;
    validate(&occurrence_ir.problem, PlanningIrLimitsV1::DEFAULT)?;
    assert!(pattern_ir.assignment_variables.is_empty());
    assert!(!occurrence_ir.assignment_variables.is_empty());
    assert_ne!(
        feature_usage(&pattern_ir.problem).required_capabilities(),
        feature_usage(&occurrence_ir.problem).required_capabilities()
    );
    assert_eq!(
        pattern_ir
            .problem
            .projections
            .iter()
            .map(|projection| (&projection.id, &projection.provenance))
            .collect::<Vec<_>>(),
        occurrence_ir
            .problem
            .projections
            .iter()
            .map(|projection| (&projection.id, &projection.provenance))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        pattern_ir
            .problem
            .provenance
            .iter()
            .map(|record| &record.id)
            .collect::<Vec<_>>(),
        occurrence_ir
            .problem
            .provenance
            .iter()
            .map(|record| &record.id)
            .collect::<Vec<_>>()
    );
    let pattern_summary = summarize(&pattern_ir.problem, PlanningIrLimitsV1::DEFAULT)?;
    let occurrence_summary = summarize(&occurrence_ir.problem, PlanningIrLimitsV1::DEFAULT)?;
    assert_eq!(pattern_summary.interval_variable_count, 2);
    assert_eq!(occurrence_summary.interval_variable_count, 0);
    assert_ne!(
        pattern_summary.canonical_ir_hash,
        occurrence_summary.canonical_ir_hash
    );
    Ok(())
}

#[test]
fn capacity_variant_is_proven_infeasible_in_both_formulations() -> Result<(), Box<dyn Error>> {
    let mut fixture = parse_fixture(include_bytes!(
        "../../../domains/school/fixtures/phase02/mini-school-infeasible-v1.json"
    ))?;
    let contract = valid_fixture()?;
    fixture.projection_ids = contract.projection_ids;
    fixture.provenance_ids = contract.provenance_ids;
    fixture.required_capabilities = contract.required_capabilities;
    assert!(
        enumerate(&fixture, Formulation::PatternChoice, false)?
            .1
            .is_empty()
    );
    assert!(
        enumerate(&fixture, Formulation::OccurrenceVariables, false)?
            .1
            .is_empty()
    );
    Ok(())
}

#[test]
fn missing_double_room_and_floating_room_fail_for_the_linking_reason() -> Result<(), Box<dyn Error>>
{
    let fixture = valid_fixture()?;
    let lecture = &fixture.occurrences[0];
    let lab = &fixture.occurrences[1];
    let lecture_room = eligible_rooms(&fixture, lecture)[0];
    let lab_room = eligible_rooms(&fixture, lab)[0];
    let valid = linked_candidate(lecture, 0, lecture_room, lab, 2, lab_room);
    assert!(validate_candidate(&fixture, &valid).is_ok());

    let mut missing = valid.clone();
    missing.meeting_selections.remove(&lab.id);
    assert_eq!(
        validate_candidate(&fixture, &missing),
        Err(CandidateFailure::MissingOccurrence)
    );
    let mut double = valid.clone();
    double
        .meeting_selections
        .get_mut(&lecture.id)
        .ok_or("missing lecture")?
        .push((0, "school.room.flex".to_owned()));
    assert_eq!(
        validate_candidate(&fixture, &double),
        Err(CandidateFailure::MultipleMeetings)
    );
    let mut floating = valid.clone();
    floating.room_claims.insert((
        "school.occurrence.unlinked".to_owned(),
        1,
        "school.room.flex".to_owned(),
    ));
    assert_eq!(
        validate_candidate(&fixture, &floating),
        Err(CandidateFailure::FloatingOrMissingRoomLink)
    );
    let mut unlinked = valid;
    unlinked
        .room_claims
        .remove(&(lecture.id.clone(), 0, lecture_room.id.clone()));
    assert_eq!(
        validate_candidate(&fixture, &unlinked),
        Err(CandidateFailure::FloatingOrMissingRoomLink)
    );
    Ok(())
}

#[test]
fn occurrence_ir_rejects_missing_double_and_unlinked_assignments() -> Result<(), Box<dyn Error>> {
    let fixture = valid_fixture()?;
    let lecture = &fixture.occurrences[0];
    let lab = &fixture.occurrences[1];
    let lecture_room = fixture
        .rooms
        .iter()
        .position(|room| room.id == "school.room.lecture")
        .ok_or("missing lecture room")?;
    let lab_room = fixture
        .rooms
        .iter()
        .position(|room| room.id == "school.room.lab")
        .ok_or("missing lab room")?;
    let flex_room = fixture
        .rooms
        .iter()
        .position(|room| room.id == "school.room.flex")
        .ok_or("missing flex room")?;
    let encoding = build_problem(&fixture, Formulation::OccurrenceVariables, false)?;
    assert_eq!(
        encoding.assignment_variables.len(),
        fixture
            .occurrences
            .iter()
            .map(|occurrence| fixture.periods.len() * eligible_rooms(&fixture, occurrence).len())
            .sum::<usize>()
    );
    assert!(
        !encoding
            .assignment_variables
            .contains_key(&(lecture.id.clone(), 0, lab_room))
    );
    assert!(
        !encoding
            .assignment_variables
            .contains_key(&(lab.id.clone(), 0, lecture_room))
    );

    let valid = occurrence_candidate(&encoding, [(lecture, 0, lecture_room), (lab, 2, lab_room)])?;
    assert!(candidate_satisfies(&encoding, &valid));

    let mut missing = valid.clone();
    for ((occurrence_id, _, _), assignment) in &encoding.assignment_variables {
        if occurrence_id == &lab.id {
            missing.booleans.insert(assignment.clone(), false);
        }
    }
    assert!(!candidate_satisfies(&encoding, &missing));

    let mut double_room = valid.clone();
    double_room.booleans.insert(
        encoding.assignment_variables[&(lecture.id.clone(), 0, flex_room)].clone(),
        true,
    );
    double_room.booleans.insert(
        encoding.room_claim_variables[&(lecture.id.clone(), flex_room)].clone(),
        true,
    );
    assert!(!candidate_satisfies(&encoding, &double_room));

    let mut unlinked_assignment = valid.clone();
    unlinked_assignment.booleans.insert(
        encoding.assignment_variables[&(lecture.id.clone(), 0, lecture_room)].clone(),
        false,
    );
    unlinked_assignment.booleans.insert(
        encoding.assignment_variables[&(lecture.id.clone(), 0, flex_room)].clone(),
        true,
    );
    unlinked_assignment.integers.insert(
        encoding.room_variables[&lecture.id].clone(),
        i64::try_from(flex_room)?,
    );
    assert!(!candidate_satisfies(&encoding, &unlinked_assignment));

    let mut floating_room_claim = valid;
    floating_room_claim.booleans.insert(
        encoding.room_claim_variables[&(lecture.id.clone(), lecture_room)].clone(),
        false,
    );
    floating_room_claim.booleans.insert(
        encoding.room_claim_variables[&(lecture.id.clone(), flex_room)].clone(),
        true,
    );
    assert!(!candidate_satisfies(&encoding, &floating_room_claim));
    Ok(())
}

#[test]
fn overlap_capacity_equipment_count_and_order_counterexamples_are_causal()
-> Result<(), Box<dyn Error>> {
    let fixture = valid_fixture()?;
    let lecture = &fixture.occurrences[0];
    let lab = &fixture.occurrences[1];
    let flex = fixture
        .rooms
        .iter()
        .find(|room| room.id == "school.room.flex")
        .ok_or("missing flex room")?;
    assert_eq!(
        validate_candidate(&fixture, &linked_candidate(lecture, 1, flex, lab, 1, flex)),
        Err(CandidateFailure::NoOverlap)
    );
    assert_eq!(
        validate_candidate(&fixture, &linked_candidate(lecture, 3, flex, lab, 1, flex)),
        Err(CandidateFailure::OrderSeparation)
    );
    let lecture_only_room = fixture
        .rooms
        .iter()
        .find(|room| room.id == "school.room.lecture")
        .ok_or("missing lecture room")?;
    assert_eq!(
        validate_candidate(
            &fixture,
            &linked_candidate(lecture, 0, flex, lab, 2, lecture_only_room)
        ),
        Err(CandidateFailure::Equipment)
    );
    let mut insufficient_capacity = fixture.clone();
    insufficient_capacity.section.size = 25;
    assert_eq!(
        validate_candidate(
            &insufficient_capacity,
            &linked_candidate(lecture, 0, lecture_only_room, lab, 2, flex)
        ),
        Err(CandidateFailure::Capacity)
    );
    let mut wrong_count = fixture.clone();
    wrong_count.occurrences[0].required_count = 2;
    assert_eq!(
        validate_candidate(
            &wrong_count,
            &linked_candidate(lecture, 0, lecture_only_room, lab, 2, flex)
        ),
        Err(CandidateFailure::InvalidRequiredCount)
    );
    assert!(
        enumerate(&wrong_count, Formulation::PatternChoice, false)?
            .1
            .is_empty()
    );
    Ok(())
}

#[test]
fn school_identity_provenance_capabilities_summary_and_hash_are_stable()
-> Result<(), Box<dyn Error>> {
    let fixture = valid_fixture()?;
    let first = build_problem(&fixture, Formulation::PatternChoice, false)?;
    let second = build_problem(&fixture, Formulation::PatternChoice, false)?;
    assert_eq!(fixture.scenario_id, "school.phase02-mini");
    assert_eq!(fixture.section.id, "school.section.s1");
    assert_eq!(fixture.linked_rule.id, "school.rule.lecture-before-lab");
    assert_eq!(
        first
            .problem
            .projections
            .iter()
            .map(|projection| projection.id.as_str())
            .collect::<BTreeSet<_>>(),
        fixture.projection_ids.iter().map(String::as_str).collect()
    );
    assert_eq!(
        first
            .problem
            .provenance
            .iter()
            .map(|record| record.id.as_str())
            .collect::<BTreeSet<_>>(),
        fixture.provenance_ids.iter().map(String::as_str).collect()
    );
    let extracted = feature_usage(&first.problem)
        .required_capabilities()
        .into_iter()
        .map(capability_name)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        extracted,
        fixture
            .required_capabilities
            .iter()
            .map(String::as_str)
            .collect()
    );
    assert_eq!(
        first.problem.declared_capabilities,
        feature_usage(&first.problem).required_capabilities()
    );
    assert_eq!(
        canonical_ir_hash(&first.problem, PlanningIrLimitsV1::DEFAULT)?,
        canonical_ir_hash(&second.problem, PlanningIrLimitsV1::DEFAULT)?
    );
    assert_eq!(
        summarize(&first.problem, PlanningIrLimitsV1::DEFAULT)?,
        summarize(&second.problem, PlanningIrLimitsV1::DEFAULT)?
    );
    Ok(())
}

#[test]
fn required_school_metamorphic_relations_hold() -> Result<(), Box<dyn Error>> {
    let fixture = valid_fixture()?;
    let (encoding, baseline) = enumerate(&fixture, Formulation::OccurrenceVariables, false)?;

    let original_hash = canonical_ir_hash(&encoding.problem, PlanningIrLimitsV1::DEFAULT)?;
    let mut renamed = encoding.problem.clone();
    renamed
        .metadata
        .display_text
        .insert("scenarioName".to_owned(), "Renamed display only".to_owned());
    assert_eq!(
        original_hash,
        canonical_ir_hash(&renamed, PlanningIrLimitsV1::DEFAULT)?
    );

    let mut with_inactive_room = fixture.clone();
    with_inactive_room.rooms.push(Room {
        id: "school.room.inactive".to_owned(),
        capacity: 0,
        equipment: BTreeSet::new(),
    });
    let inactive_schedules =
        enumerate(&with_inactive_room, Formulation::OccurrenceVariables, false)?.1;
    assert_eq!(baseline, inactive_schedules);

    let mut tightened = fixture.clone();
    tightened.linked_rule.minimum_separation = tightened
        .linked_rule
        .minimum_separation
        .checked_add(1)
        .ok_or("separation overflow")?;
    let tightened_schedules = enumerate(&tightened, Formulation::OccurrenceVariables, false)?.1;
    assert!(tightened_schedules.is_subset(&baseline));

    let required_schedules = enumerate(&fixture, Formulation::OccurrenceVariables, true)?.1;
    assert!(required_schedules.is_subset(&baseline));
    assert!(required_schedules.len() < baseline.len());
    Ok(())
}
