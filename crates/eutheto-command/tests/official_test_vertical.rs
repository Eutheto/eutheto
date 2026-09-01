use eutheto_command::{OFFICIAL_TEST_PACK_ID, OfficialTestPack};
use eutheto_domain_api::{
    CompileContext, DomainBatchCommand, DomainCatalog, DomainExplanation, DomainMutation,
    DomainPack, DomainPackDescriptor, DomainPackError, DomainShareResult, DomainValidationReport,
    HistoricalPortableDomainDocument, PortableDomainDocument, PortableImportContext,
    ShareResultOptions,
};
use eutheto_domain_ir::{
    AcceptedResult, AssignmentValue, DomainAssignmentId, DomainEntityId, DomainEntityKindId,
    DomainEntityRef, NormalizedSolution, OptimizationDirection, ScoreCategoryId, ScoreLevelId,
    ScoreLevelValue, ScoreVector, VERIFICATION_REPORT_SCHEMA_VERSION, VerificationIssue,
    VerificationIssueId, VerificationReport, VerificationSeverity,
};
use eutheto_planning_ir::{
    BoolVariable, BoolVariableId, CandidateValues, Capability, ComparisonOp, CompilerId,
    Constraint, ConstraintRecord, ConstraintTag, InclusiveRange, IntDomain, IntVariable,
    IntVariableId, IntervalVariable, IntervalVariableId, LinearComparison, LinearExpression,
    LinearTerm, Literal, MetadataKey, ObjectiveLevel, ObjectiveLevelId, ObjectivePlan,
    ObjectiveTerm, ObjectiveTermId, ObjectiveTermKind, PLANNING_IR_SCHEMA_VERSION,
    PROJECTION_SCHEMA_VERSION, PlanningConstraintId, PlanningIrLimitsV1, PlanningMetadata,
    PlanningProblem, ProjectionExpression, ProjectionId, ProvenanceId, ProvenanceParameter,
    ProvenanceRecord, ProvenanceSourceKind, SolutionProjection, Variable, feature_usage, summarize,
    validate,
};
use eutheto_types::{CancellationToken, ScenarioDocument, SolutionId};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::str::FromStr;

const SCENARIO_ID: &str = "0195a5e4-7c00-7000-8000-000000000011";
const SOLUTION_ID: &str = "0195a5e4-7c00-7000-8000-000000000012";
const FIXTURE_ENTITY_KIND: &str = "official.test.fixture.option";
const FIXTURE_SCORE_LEVEL: &str = "official.test.fixture.score.selected";
const FIXTURE_SCORE_CATEGORY: &str = "official.test.fixture.score.selected-total";
const MAX_FIXTURE_OPTIONS: u32 = 8;

#[derive(Clone, Copy, Debug)]
struct FixtureOption {
    key: &'static str,
    start: i64,
    duration: i64,
    eligible: bool,
}

impl FixtureOption {
    fn end(self) -> i64 {
        self.start + self.duration
    }
}

const OPTIONS: [FixtureOption; 3] = [
    FixtureOption {
        key: "early",
        start: 0,
        duration: 2,
        eligible: true,
    },
    FixtureOption {
        key: "overlap",
        start: 1,
        duration: 2,
        eligible: true,
    },
    FixtureOption {
        key: "ineligible",
        start: 3,
        duration: 1,
        eligible: false,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ModelEstimate {
    variables: u32,
    constraints: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PruningSummary {
    candidates_before: u32,
    candidates_after: u32,
    model_before: ModelEstimate,
    model_after: ModelEstimate,
}

impl PruningSummary {
    fn new(before: &[FixtureOption], after: &[FixtureOption]) -> Result<Self, DomainPackError> {
        let candidates_before = bounded_count(before.len())?;
        let candidates_after = bounded_count(after.len())?;
        if candidates_before == 0 || candidates_after == 0 || candidates_after > candidates_before {
            return Err(contract("invalid synthetic pruning counts"));
        }
        Ok(Self {
            candidates_before,
            candidates_after,
            model_before: estimate(before)?,
            model_after: estimate(after)?,
        })
    }

    fn metadata(self) -> Result<BTreeMap<MetadataKey, ProvenanceParameter>, DomainPackError> {
        let entries = [
            (
                "official_test.pruning.candidates_before",
                self.candidates_before,
            ),
            (
                "official_test.pruning.candidates_after",
                self.candidates_after,
            ),
            (
                "official_test.pruning.model_variables_before",
                self.model_before.variables,
            ),
            (
                "official_test.pruning.model_variables_after",
                self.model_after.variables,
            ),
            (
                "official_test.pruning.model_constraints_before",
                self.model_before.constraints,
            ),
            (
                "official_test.pruning.model_constraints_after",
                self.model_after.constraints,
            ),
        ];
        entries
            .into_iter()
            .map(|(key, value)| {
                Ok((
                    MetadataKey::new(key).map_err(contract)?,
                    ProvenanceParameter::Integer(i64::from(value)),
                ))
            })
            .collect()
    }

    fn read(problem: &PlanningProblem) -> Result<Self, DomainPackError> {
        Ok(Self {
            candidates_before: metadata_count(problem, "official_test.pruning.candidates_before")?,
            candidates_after: metadata_count(problem, "official_test.pruning.candidates_after")?,
            model_before: ModelEstimate {
                variables: metadata_count(problem, "official_test.pruning.model_variables_before")?,
                constraints: metadata_count(
                    problem,
                    "official_test.pruning.model_constraints_before",
                )?,
            },
            model_after: ModelEstimate {
                variables: metadata_count(problem, "official_test.pruning.model_variables_after")?,
                constraints: metadata_count(
                    problem,
                    "official_test.pruning.model_constraints_after",
                )?,
            },
        })
    }
}

fn bounded_count(count: usize) -> Result<u32, DomainPackError> {
    let count = u32::try_from(count).map_err(contract)?;
    if count > MAX_FIXTURE_OPTIONS {
        return Err(contract("synthetic option corpus exceeds its fixed bound"));
    }
    Ok(count)
}

fn estimate(options: &[FixtureOption]) -> Result<ModelEstimate, DomainPackError> {
    let count = bounded_count(options.len())?;
    let ineligible = bounded_count(options.iter().filter(|option| !option.eligible).count())?;
    Ok(ModelEstimate {
        variables: count
            .checked_mul(6)
            .ok_or_else(|| contract("synthetic variable estimate overflow"))?,
        constraints: count
            .checked_mul(5)
            .and_then(|value| value.checked_add(2))
            .and_then(|value| value.checked_add(ineligible))
            .ok_or_else(|| contract("synthetic constraint estimate overflow"))?,
    })
}

fn metadata_count(problem: &PlanningProblem, key: &str) -> Result<u32, DomainPackError> {
    let key = MetadataKey::new(key).map_err(contract)?;
    let Some(ProvenanceParameter::Integer(value)) = problem.metadata.compile_metadata.get(&key)
    else {
        return Err(contract("missing typed pruning metadata"));
    };
    u32::try_from(*value).map_err(contract)
}

#[derive(Clone, Copy)]
struct IntervalFixturePack {
    prune: bool,
}

impl DomainPack for IntervalFixturePack {
    fn descriptor(&self) -> Result<DomainPackDescriptor, DomainPackError> {
        OfficialTestPack.descriptor()
    }

    fn catalog(&self) -> Result<DomainCatalog, DomainPackError> {
        OfficialTestPack.catalog()
    }

    fn new_document(&self, shell: ScenarioDocument) -> Result<ScenarioDocument, DomainPackError> {
        OfficialTestPack.new_document(shell)
    }

    fn migrate_document(
        &self,
        document: ScenarioDocument,
    ) -> Result<ScenarioDocument, DomainPackError> {
        OfficialTestPack.migrate_document(document)
    }

    fn validate_fast(&self, document: &ScenarioDocument) -> DomainValidationReport {
        OfficialTestPack.validate_fast(document)
    }

    fn validate_full(&self, document: &ScenarioDocument) -> DomainValidationReport {
        OfficialTestPack.validate_full(document)
    }

    fn apply_batch(
        &self,
        document: &ScenarioDocument,
        batch: &DomainBatchCommand,
    ) -> Result<DomainMutation, DomainPackError> {
        OfficialTestPack.apply_batch(document, batch)
    }

    fn compile(
        &self,
        document: &ScenarioDocument,
        context: &CompileContext,
    ) -> Result<PlanningProblem, DomainPackError> {
        build_interval_problem(document, context, self.prune)
    }

    fn project(
        &self,
        problem: &PlanningProblem,
        candidate: &CandidateValues,
        solution_id: SolutionId,
    ) -> Result<NormalizedSolution, DomainPackError> {
        OfficialTestPack.project(problem, candidate, solution_id)
    }

    fn verify(
        &self,
        document: &ScenarioDocument,
        solution: &NormalizedSolution,
    ) -> Result<VerificationReport, DomainPackError> {
        let (score, issues) = assess_interval_fixture(document, solution)?;
        let report = VerificationReport {
            schema_version: VERIFICATION_REPORT_SCHEMA_VERSION,
            scenario_revision: solution.scenario_revision,
            feasible: issues.is_empty(),
            issues,
            score: Some(score),
        };
        report.validate().map_err(contract)?;
        Ok(report)
    }

    fn score(
        &self,
        document: &ScenarioDocument,
        solution: &NormalizedSolution,
    ) -> Result<ScoreVector, DomainPackError> {
        assess_interval_fixture(document, solution).map(|(score, _)| score)
    }

    fn export_portable(
        &self,
        document: &ScenarioDocument,
    ) -> Result<PortableDomainDocument, DomainPackError> {
        OfficialTestPack.export_portable(document)
    }

    fn migrate_portable(
        &self,
        document: HistoricalPortableDomainDocument,
    ) -> Result<PortableDomainDocument, DomainPackError> {
        OfficialTestPack.migrate_portable(document)
    }

    fn import_portable(
        &self,
        document: &PortableDomainDocument,
        context: &PortableImportContext,
    ) -> Result<ScenarioDocument, DomainPackError> {
        OfficialTestPack.import_portable(document, context)
    }

    fn build_share_result(
        &self,
        document: &ScenarioDocument,
        accepted: &AcceptedResult,
        options: ShareResultOptions,
    ) -> Result<DomainShareResult, DomainPackError> {
        OfficialTestPack.build_share_result(document, accepted, options)
    }

    fn build_view(
        &self,
        document: &ScenarioDocument,
        solution: Option<&NormalizedSolution>,
        view_id: &str,
    ) -> Result<eutheto_domain_api::DomainView, DomainPackError> {
        OfficialTestPack.build_view(document, solution, view_id)
    }

    fn explain(
        &self,
        document: &ScenarioDocument,
        solution: Option<&NormalizedSolution>,
        request_id: &str,
    ) -> Result<DomainExplanation, DomainPackError> {
        OfficialTestPack.explain(document, solution, request_id)
    }
}

// This synthetic IR fixture is intentionally kept in one place so the variable,
// constraint, projection, and provenance correspondence remains auditable.
#[allow(clippy::too_many_lines)]
fn build_interval_problem(
    document: &ScenarioDocument,
    context: &CompileContext,
    prune: bool,
) -> Result<PlanningProblem, DomainPackError> {
    if context.cancellation.is_cancelled() {
        return Err(DomainPackError::Cancelled);
    }
    if document.domain_pack.id.as_str() != OFFICIAL_TEST_PACK_ID
        || document.domain_pack.schema_version != 1
    {
        return Err(contract("optional fixture document pack mismatch"));
    }

    let included: Vec<_> = OPTIONS
        .iter()
        .copied()
        .filter(|option| !prune || option.eligible)
        .collect();
    let pruning = PruningSummary::new(&OPTIONS, &included)?;
    let global_rule = ProvenanceId::new("official_test.fixture.rule.global").map_err(contract)?;
    let preference =
        ProvenanceId::new("official_test.fixture.preference.selected").map_err(contract)?;
    let mut variables = Vec::with_capacity(included.len() * 6);
    let mut constraints = Vec::with_capacity(pruning.model_after.constraints as usize);
    let mut objective_terms = Vec::with_capacity(included.len());
    let mut projections = Vec::with_capacity(OPTIONS.len());
    let mut provenance = Vec::with_capacity(OPTIONS.len() * 3 + 2);
    let mut interval_ids = Vec::with_capacity(included.len());

    for option in OPTIONS {
        let entity = fixture_entity(option)?;
        let fact = fixture_provenance("fact", option)?;
        let eligibility_rule = fixture_provenance("eligibility", option)?;
        let projection = fixture_provenance("projection", option)?;
        provenance.extend([
            provenance_record(
                fact.clone(),
                ProvenanceSourceKind::Fact,
                format!("official.test.fixture.option.{}", option.key),
                entity.clone(),
            ),
            provenance_record(
                eligibility_rule.clone(),
                ProvenanceSourceKind::RequiredRule,
                format!("official.test.fixture.eligibility.{}", option.key),
                entity.clone(),
            ),
            provenance_record(
                projection.clone(),
                ProvenanceSourceKind::Projection,
                format!("official.test.fixture.projection.{}", option.key),
                entity.clone(),
            ),
        ]);

        if !included.iter().any(|included| included.key == option.key) {
            projections.push(SolutionProjection {
                id: fixture_projection_id(option)?,
                assignment_id: fixture_assignment_id(option)?,
                entity,
                required: true,
                expression: ProjectionExpression::Constant(AssignmentValue::Absent),
                provenance: projection,
            });
            continue;
        }

        let present = fixture_bool_id(option)?;
        let start = fixture_int_id(option, "start")?;
        let duration = fixture_int_id(option, "duration")?;
        let end = fixture_int_id(option, "end")?;
        let cost = fixture_int_id(option, "cost")?;
        let interval = fixture_interval_id(option)?;
        variables.extend([
            Variable::Boolean(BoolVariable {
                id: present.clone(),
                provenance: fact.clone(),
            }),
            Variable::Integer(IntVariable {
                id: start.clone(),
                domain: IntDomain::new(vec![InclusiveRange { start: 0, end: 4 }])
                    .map_err(contract)?,
                provenance: fact.clone(),
            }),
            Variable::Integer(IntVariable {
                id: duration.clone(),
                domain: IntDomain::new(vec![InclusiveRange { start: 1, end: 2 }])
                    .map_err(contract)?,
                provenance: fact.clone(),
            }),
            Variable::Integer(IntVariable {
                id: end.clone(),
                domain: IntDomain::new(vec![InclusiveRange { start: 1, end: 5 }])
                    .map_err(contract)?,
                provenance: fact.clone(),
            }),
            Variable::Integer(IntVariable {
                id: cost.clone(),
                domain: IntDomain::new(vec![InclusiveRange { start: 0, end: 1 }])
                    .map_err(contract)?,
                provenance: fact.clone(),
            }),
            Variable::Interval(IntervalVariable {
                id: interval.clone(),
                start: start.clone(),
                duration: duration.clone(),
                end: end.clone(),
                presence: Some(Literal::positive(present.clone())),
                provenance: fact.clone(),
            }),
        ]);
        interval_ids.push(interval.clone());
        constraints.extend([
            fixed_integer_constraint(option, "start", start, option.start, &present, true, &fact)?,
            fixed_integer_constraint(
                option,
                "duration",
                duration,
                option.duration,
                &present,
                true,
                &fact,
            )?,
            fixed_integer_constraint(option, "end", end, option.end(), &present, true, &fact)?,
            fixed_integer_constraint(
                option,
                "selected_cost",
                cost.clone(),
                1,
                &present,
                true,
                &fact,
            )?,
            fixed_integer_constraint(
                option,
                "absent_cost",
                cost.clone(),
                0,
                &present,
                false,
                &fact,
            )?,
        ]);
        if !option.eligible {
            constraints.push(ConstraintRecord {
                id: PlanningConstraintId::new(format!(
                    "official_test.fixture.required_eligibility.{}",
                    option.key
                ))
                .map_err(contract)?,
                body: Constraint::bool_and(vec![Literal::negative(present.clone())]),
                enforcement: Vec::new(),
                provenance: eligibility_rule,
                tags: vec![ConstraintTag::new("official_test.fixture.required").map_err(contract)?],
            });
        }
        objective_terms.push(ObjectiveTerm {
            id: ObjectiveTermId::new(format!(
                "official_test.fixture.objective.selected.{}",
                option.key
            ))
            .map_err(contract)?,
            expression: LinearExpression::new(
                vec![LinearTerm {
                    variable: cost,
                    coefficient: 1,
                }],
                0,
            )
            .map_err(contract)?,
            kind: ObjectiveTermKind::Penalty,
            category: ScoreCategoryId::new(FIXTURE_SCORE_CATEGORY).map_err(contract)?,
            provenance: preference.clone(),
        });
        projections.push(SolutionProjection {
            id: fixture_projection_id(option)?,
            assignment_id: fixture_assignment_id(option)?,
            entity,
            required: true,
            expression: ProjectionExpression::Interval(interval),
            provenance: projection,
        });
    }

    provenance.extend([
        ProvenanceRecord {
            id: global_rule.clone(),
            source_kind: ProvenanceSourceKind::RequiredRule,
            source_id: "official.test.fixture.interval-capacity".to_owned(),
            entity_refs: OPTIONS
                .into_iter()
                .map(fixture_entity)
                .collect::<Result<Vec<_>, _>>()?,
            message_key: "official.test.fixture.provenance.interval_capacity".to_owned(),
            parameters: BTreeMap::new(),
            parent: None,
        },
        ProvenanceRecord {
            id: preference.clone(),
            source_kind: ProvenanceSourceKind::Preference,
            source_id: "official.test.fixture.preference.selected".to_owned(),
            entity_refs: Vec::new(),
            message_key: "official.test.fixture.provenance.selected_preference".to_owned(),
            parameters: BTreeMap::new(),
            parent: None,
        },
    ]);
    constraints.extend([
        ConstraintRecord {
            id: PlanningConstraintId::new("official_test.fixture.no_overlap").map_err(contract)?,
            body: Constraint::no_overlap(interval_ids.clone()),
            enforcement: Vec::new(),
            provenance: global_rule.clone(),
            tags: vec![ConstraintTag::new("official_test.fixture.required").map_err(contract)?],
        },
        ConstraintRecord {
            id: PlanningConstraintId::new("official_test.fixture.cumulative").map_err(contract)?,
            body: Constraint::cumulative(interval_ids.clone(), vec![1; interval_ids.len()], 1)
                .map_err(contract)?,
            enforcement: Vec::new(),
            provenance: global_rule,
            tags: vec![ConstraintTag::new("official_test.fixture.required").map_err(contract)?],
        },
    ]);

    let mut problem = PlanningProblem {
        schema_version: PLANNING_IR_SCHEMA_VERSION,
        variables,
        constraints,
        objectives: ObjectivePlan {
            levels: vec![ObjectiveLevel {
                id: ObjectiveLevelId::new(FIXTURE_SCORE_LEVEL).map_err(contract)?,
                direction: OptimizationDirection::Minimize,
                lower_bound: 0,
                upper_bound: i64::from(pruning.candidates_after),
                terms: objective_terms,
                provenance: preference,
            }],
        },
        assumptions: Vec::new(),
        projections,
        provenance,
        metadata: PlanningMetadata {
            pack_id: document.domain_pack.id.clone(),
            scenario_id: document.scenario_id,
            scenario_revision: context.scenario_revision,
            projection_version: PROJECTION_SCHEMA_VERSION,
            compiler_id: CompilerId::new("official_test.fixture.interval_compiler")
                .map_err(contract)?,
            compiler_version: "1.0.0".to_owned(),
            compile_metadata: pruning.metadata()?,
            display_text: BTreeMap::new(),
        },
        declared_capabilities: BTreeSet::new(),
        split_authorization: None,
    };
    problem.declared_capabilities = feature_usage(&problem).required_capabilities();
    problem.canonicalize().map_err(contract)?;
    validate(&problem, context.planning_limits).map_err(contract)?;
    Ok(problem)
}

fn fixed_integer_constraint(
    option: FixtureOption,
    label: &str,
    variable: IntVariableId,
    value: i64,
    present: &BoolVariableId,
    when_present: bool,
    provenance: &ProvenanceId,
) -> Result<ConstraintRecord, DomainPackError> {
    Ok(ConstraintRecord {
        id: PlanningConstraintId::new(format!(
            "official_test.fixture.fixed_{label}.{}",
            option.key
        ))
        .map_err(contract)?,
        body: Constraint::LinearComparison(LinearComparison {
            expression: LinearExpression::new(
                vec![LinearTerm {
                    variable,
                    coefficient: 1,
                }],
                0,
            )
            .map_err(contract)?,
            op: ComparisonOp::Equal,
            rhs: value,
        }),
        enforcement: vec![if when_present {
            Literal::positive(present.clone())
        } else {
            Literal::negative(present.clone())
        }],
        provenance: provenance.clone(),
        tags: vec![ConstraintTag::new("official_test.fixture.required").map_err(contract)?],
    })
}

fn assess_interval_fixture(
    document: &ScenarioDocument,
    solution: &NormalizedSolution,
) -> Result<(ScoreVector, Vec<VerificationIssue>), DomainPackError> {
    if document.domain_pack.id.as_str() != OFFICIAL_TEST_PACK_ID
        || solution.pack_id != document.domain_pack.id
        || solution.scenario_id != document.scenario_id
    {
        return Err(contract("optional fixture solution scenario mismatch"));
    }
    solution.validate().map_err(contract)?;
    let expected_ids: BTreeSet<_> = OPTIONS
        .into_iter()
        .map(fixture_assignment_id)
        .collect::<Result<_, _>>()?;
    if solution.assignments.len() != expected_ids.len()
        || solution
            .assignments
            .iter()
            .any(|assignment| !expected_ids.contains(&assignment.id))
    {
        return Err(contract("optional fixture assignment set mismatch"));
    }

    let assignments: BTreeMap<_, _> = solution
        .assignments
        .iter()
        .map(|assignment| (assignment.id.clone(), assignment))
        .collect();
    let mut selected = Vec::new();
    let mut issues = Vec::new();
    for option in OPTIONS {
        let assignment = assignments
            .get(&fixture_assignment_id(option)?)
            .ok_or_else(|| contract("optional fixture assignment is missing"))?;
        if assignment.entity != fixture_entity(option)? {
            issues.push(fixture_issue("identity", option.key, &[option])?);
            continue;
        }
        match &assignment.value {
            AssignmentValue::Absent => {}
            AssignmentValue::Interval(interval)
                if interval.start == option.start
                    && interval.duration == option.duration
                    && interval.end == option.end() =>
            {
                if option.eligible {
                    selected.push((option, *interval));
                } else {
                    issues.push(fixture_issue("eligibility", option.key, &[option])?);
                }
            }
            AssignmentValue::Interval(_) => {
                issues.push(fixture_issue("interval_values", option.key, &[option])?);
            }
            AssignmentValue::Boolean(_) | AssignmentValue::Integer(_) => {
                issues.push(fixture_issue("assignment_type", option.key, &[option])?);
            }
        }
    }

    for left_index in 0..selected.len() {
        for right_index in (left_index + 1)..selected.len() {
            let (left_option, left) = selected[left_index];
            let (right_option, right) = selected[right_index];
            if left.start < right.end && right.start < left.end {
                let key = format!("{}_{}", left_option.key, right_option.key);
                issues.push(fixture_issue(
                    "no_overlap",
                    &key,
                    &[left_option, right_option],
                )?);
            }
        }
    }
    for instant in 0..5 {
        let demand = selected
            .iter()
            .filter(|(_, interval)| interval.start <= instant && instant < interval.end)
            .count();
        if demand > 1 {
            issues.push(fixture_issue(
                "cumulative",
                &instant.to_string(),
                &selected
                    .iter()
                    .map(|(option, _)| *option)
                    .collect::<Vec<_>>(),
            )?);
            break;
        }
    }
    issues.sort_by(|left, right| left.id.cmp(&right.id));
    let selected_count = i64::try_from(selected.len()).map_err(contract)?;
    let score = fixture_score(selected_count, issues.len())?;
    Ok((score, issues))
}

fn fixture_score(selected_count: i64, issue_count: usize) -> Result<ScoreVector, DomainPackError> {
    Ok(ScoreVector {
        feasibility: i64::try_from(issue_count).map_err(contract)?,
        levels: vec![ScoreLevelValue {
            level_id: ScoreLevelId::new(FIXTURE_SCORE_LEVEL).map_err(contract)?,
            value: selected_count,
            direction: OptimizationDirection::Minimize,
            category_breakdown: [(
                ScoreCategoryId::new(FIXTURE_SCORE_CATEGORY).map_err(contract)?,
                selected_count,
            )]
            .into_iter()
            .collect(),
        }],
    })
}

fn fixture_issue(
    kind: &str,
    suffix: &str,
    options: &[FixtureOption],
) -> Result<VerificationIssue, DomainPackError> {
    Ok(VerificationIssue {
        id: VerificationIssueId::new(format!("official.test.fixture.verify.{kind}.{suffix}"))
            .map_err(contract)?,
        severity: VerificationSeverity::Error,
        message_key: format!("official.test.fixture.verify.{kind}"),
        entities: options
            .iter()
            .copied()
            .map(fixture_entity)
            .collect::<Result<_, _>>()?,
        evidence: Vec::new(),
    })
}

fn fixture_entity(option: FixtureOption) -> Result<DomainEntityRef, DomainPackError> {
    Ok(DomainEntityRef {
        kind: DomainEntityKindId::new(FIXTURE_ENTITY_KIND).map_err(contract)?,
        id: DomainEntityId::new(format!("official.test.fixture.option.{}", option.key))
            .map_err(contract)?,
    })
}

fn fixture_provenance(kind: &str, option: FixtureOption) -> Result<ProvenanceId, DomainPackError> {
    ProvenanceId::new(format!("official_test.fixture.{kind}.{}", option.key)).map_err(contract)
}

fn fixture_bool_id(option: FixtureOption) -> Result<BoolVariableId, DomainPackError> {
    BoolVariableId::new(format!("official_test.fixture.present.{}", option.key)).map_err(contract)
}

fn fixture_int_id(
    option: FixtureOption,
    component: &str,
) -> Result<IntVariableId, DomainPackError> {
    IntVariableId::new(format!("official_test.fixture.{component}.{}", option.key))
        .map_err(contract)
}

fn fixture_interval_id(option: FixtureOption) -> Result<IntervalVariableId, DomainPackError> {
    IntervalVariableId::new(format!("official_test.fixture.interval.{}", option.key))
        .map_err(contract)
}

fn fixture_projection_id(option: FixtureOption) -> Result<ProjectionId, DomainPackError> {
    ProjectionId::new(format!("official_test.fixture.project.{}", option.key)).map_err(contract)
}

fn fixture_assignment_id(option: FixtureOption) -> Result<DomainAssignmentId, DomainPackError> {
    DomainAssignmentId::new(format!("official.test.fixture.assignment.{}", option.key))
        .map_err(contract)
}

fn provenance_record(
    id: ProvenanceId,
    source_kind: ProvenanceSourceKind,
    source_id: String,
    entity: DomainEntityRef,
) -> ProvenanceRecord {
    ProvenanceRecord {
        id,
        source_kind,
        source_id,
        entity_refs: vec![entity],
        message_key: "official.test.fixture.provenance.option".to_owned(),
        parameters: BTreeMap::new(),
        parent: None,
    }
}

fn fixture_document() -> Result<ScenarioDocument, serde_json::Error> {
    serde_json::from_value(json!({
        "format": "eutheto/scenario",
        "formatVersion": 1,
        "scenarioId": SCENARIO_ID,
        "domainPack": { "id": OFFICIAL_TEST_PACK_ID, "schemaVersion": 1 },
        "metadata": {
            "title": "Optional interval contract fixture",
            "description": "",
            "createdAt": "2026-08-29T12:00:00Z",
            "updatedAt": "2026-08-29T12:00:00Z"
        },
        "settings": {
            "timeZone": "UTC",
            "locale": "en-US",
            "units": "metric",
            "horizon": {
                "start": "2026-08-29T12:00:00Z",
                "end": "2026-09-05T12:00:00Z"
            },
            "gapPolicy": "reject",
            "overlapPolicy": "earlier"
        },
        "domain": {
            "entities": {},
            "rules": {},
            "preferences": {},
            "lockedAssignments": {}
        },
        "extensions": {}
    }))
}

fn fixture_context() -> CompileContext {
    CompileContext {
        scenario_revision: 11,
        semantic_metadata: BTreeMap::new(),
        cancellation: CancellationToken::new(),
        planning_limits: PlanningIrLimitsV1::DEFAULT,
    }
}

fn candidate_for(
    problem: &PlanningProblem,
    selected_mask: usize,
) -> Result<CandidateValues, DomainPackError> {
    let bool_ids: BTreeSet<_> = problem
        .variables
        .iter()
        .filter_map(|variable| match variable {
            Variable::Boolean(value) => Some(value.id.clone()),
            Variable::Integer(_) | Variable::Interval(_) => None,
        })
        .collect();
    let int_ids: BTreeSet<_> = problem
        .variables
        .iter()
        .filter_map(|variable| match variable {
            Variable::Integer(value) => Some(value.id.clone()),
            Variable::Boolean(_) | Variable::Interval(_) => None,
        })
        .collect();
    let mut candidate = CandidateValues::default();
    for (index, option) in OPTIONS.into_iter().enumerate() {
        let selected = selected_mask & (1 << index) != 0;
        let present = fixture_bool_id(option)?;
        if bool_ids.contains(&present) {
            candidate.booleans.insert(present, selected);
        }
        for (component, value) in [
            ("start", option.start),
            ("duration", option.duration),
            ("end", option.end()),
            ("cost", i64::from(selected)),
        ] {
            let id = fixture_int_id(option, component)?;
            if int_ids.contains(&id) {
                candidate.integers.insert(id, value);
            }
        }
    }
    Ok(candidate)
}

fn projected_schedule(solution: &NormalizedSolution) -> Result<String, serde_json::Error> {
    serde_json::to_string(&solution.assignments)
}

fn contract(error: impl std::fmt::Display) -> DomainPackError {
    DomainPackError::Contract(error.to_string())
}

fn compile_and_assert_interval_problem(
    pack: IntervalFixturePack,
    document: &ScenarioDocument,
    context: &CompileContext,
) -> Result<PlanningProblem, Box<dyn Error>> {
    let first = pack.compile(document, context)?;
    let second = pack.compile(document, context)?;
    validate(&first, context.planning_limits)?;
    let summary = summarize(&first, context.planning_limits)?;
    assert_eq!(first, second);
    assert_eq!(summary, summarize(&second, context.planning_limits)?);
    assert_eq!(summary.interval_variable_count, 3);
    assert_eq!(summary.manifest.optional_interval_count, 3);
    assert_eq!(summary.manifest.global_interval_constraint_count, 2);
    assert_eq!(
        summary.manifest.required_capabilities(),
        [
            Capability::BoolAnd,
            Capability::LinearComparison,
            Capability::NoOverlap,
            Capability::Cumulative,
            Capability::OptionalIntervals,
            Capability::ObjectivePenalty,
            Capability::IntervalProjection,
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(
        first.declared_capabilities,
        summary.manifest.required_capabilities()
    );
    Ok(first)
}

#[test]
fn optional_intervals_compile_project_and_verify_boundaries() -> Result<(), Box<dyn Error>> {
    let pack = IntervalFixturePack { prune: false };
    let document = fixture_document()?;
    let context = fixture_context();
    let first = compile_and_assert_interval_problem(pack, &document, &context)?;

    let missing_presence = CandidateValues::default();
    assert!(
        pack.project(
            &first,
            &missing_presence,
            SolutionId::from_str(SOLUTION_ID)?
        )
        .is_err()
    );

    let absent_candidate = candidate_for(&first, 0)?;
    let absent = pack.project(
        &first,
        &absent_candidate,
        SolutionId::from_str(SOLUTION_ID)?,
    )?;
    assert!(
        absent
            .assignments
            .iter()
            .all(|assignment| assignment.value == AssignmentValue::Absent)
    );
    let absent_report = pack.verify(&document, &absent)?;
    assert!(absent_report.feasible);
    assert_eq!(
        absent_report
            .score
            .as_ref()
            .map(|score| score.levels[0].value),
        Some(0)
    );

    let mut ignored_incoherence = absent_candidate;
    ignored_incoherence
        .integers
        .insert(fixture_int_id(OPTIONS[0], "end")?, 3);
    let still_absent = pack.project(
        &first,
        &ignored_incoherence,
        SolutionId::from_str(SOLUTION_ID)?,
    )?;
    assert!(
        still_absent
            .assignments
            .iter()
            .all(|assignment| assignment.value == AssignmentValue::Absent)
    );

    let present_candidate = candidate_for(&first, 1)?;
    let present = pack.project(
        &first,
        &present_candidate,
        SolutionId::from_str(SOLUTION_ID)?,
    )?;
    let present_report = pack.verify(&document, &present)?;
    assert!(present_report.feasible);
    let independently_scored = pack.score(&document, &present)?;
    assert_eq!(present_report.score.as_ref(), Some(&independently_scored));
    assert!(matches!(
        &present.assignments[0].value,
        AssignmentValue::Interval(interval)
            if interval.start == 0 && interval.duration == 2 && interval.end == 2
    ));

    let mut incoherent = present_candidate;
    incoherent
        .integers
        .insert(fixture_int_id(OPTIONS[0], "end")?, 3);
    let incoherent_error = pack.project(&first, &incoherent, SolutionId::from_str(SOLUTION_ID)?);
    let message = match incoherent_error {
        Err(DomainPackError::Contract(message)) => message,
        other => {
            return Err(format!(
                "a present incoherent interval must fail projection with a contract error, got {other:?}"
            )
            .into());
        }
    };
    assert!(message.contains("InvalidInterval"));

    let overlap = pack.project(
        &first,
        &candidate_for(&first, 0b011)?,
        SolutionId::from_str(SOLUTION_ID)?,
    )?;
    let overlap_report = pack.verify(&document, &overlap)?;
    assert!(!overlap_report.feasible);
    assert!(
        overlap_report
            .issues
            .iter()
            .any(|issue| issue.message_key == "official.test.fixture.verify.no_overlap")
    );
    assert!(
        overlap_report
            .issues
            .iter()
            .any(|issue| issue.message_key == "official.test.fixture.verify.cumulative")
    );
    Ok(())
}

#[test]
fn deterministic_pruning_preserves_every_feasible_schedule_and_score() -> Result<(), Box<dyn Error>>
{
    let document = fixture_document()?;
    let context = fixture_context();
    let unpruned_pack = IntervalFixturePack { prune: false };
    let pruned_pack = IntervalFixturePack { prune: true };
    let unpruned = unpruned_pack.compile(&document, &context)?;
    let pruned = pruned_pack.compile(&document, &context)?;
    assert_eq!(unpruned, unpruned_pack.compile(&document, &context)?);
    assert_eq!(pruned, pruned_pack.compile(&document, &context)?);
    validate(&unpruned, context.planning_limits)?;
    validate(&pruned, context.planning_limits)?;

    let unpruned_summary = summarize(&unpruned, context.planning_limits)?;
    let pruned_summary = summarize(&pruned, context.planning_limits)?;
    let unpruned_pruning = PruningSummary::read(&unpruned)?;
    let pruned_pruning = PruningSummary::read(&pruned)?;
    assert_eq!(
        unpruned_pruning,
        PruningSummary {
            candidates_before: 3,
            candidates_after: 3,
            model_before: ModelEstimate {
                variables: 18,
                constraints: 18,
            },
            model_after: ModelEstimate {
                variables: 18,
                constraints: 18,
            },
        }
    );
    assert_eq!(
        pruned_pruning,
        PruningSummary {
            candidates_before: 3,
            candidates_after: 2,
            model_before: ModelEstimate {
                variables: 18,
                constraints: 18,
            },
            model_after: ModelEstimate {
                variables: 12,
                constraints: 12,
            },
        }
    );
    assert_eq!(
        pruned_summary.variable_count,
        u64::from(pruned_pruning.model_after.variables)
    );
    assert_eq!(
        pruned_summary.constraint_count,
        u64::from(pruned_pruning.model_after.constraints)
    );
    assert!(pruned.variables.iter().all(|variable| match variable {
        Variable::Boolean(value) => !value.id.as_str().contains("ineligible"),
        Variable::Integer(value) => !value.id.as_str().contains("ineligible"),
        Variable::Interval(value) => !value.id.as_str().contains("ineligible"),
    }));
    assert_ne!(
        unpruned_summary.canonical_ir_hash,
        pruned_summary.canonical_ir_hash
    );
    assert_eq!(
        unpruned
            .projections
            .iter()
            .map(|projection| (&projection.assignment_id, &projection.entity))
            .collect::<Vec<_>>(),
        pruned
            .projections
            .iter()
            .map(|projection| (&projection.assignment_id, &projection.entity))
            .collect::<Vec<_>>()
    );
    assert_eq!(unpruned.provenance, pruned.provenance);

    let mut unpruned_feasible = BTreeMap::new();
    let mut pruned_feasible = BTreeMap::new();
    for mask in 0..(1 << OPTIONS.len()) {
        for (pack, problem, feasible) in [
            (unpruned_pack, &unpruned, &mut unpruned_feasible),
            (pruned_pack, &pruned, &mut pruned_feasible),
        ] {
            let solution = pack.project(
                problem,
                &candidate_for(problem, mask)?,
                SolutionId::from_str(SOLUTION_ID)?,
            )?;
            let report = pack.verify(&document, &solution)?;
            let score = pack.score(&document, &solution)?;
            assert_eq!(report.score.as_ref(), Some(&score));
            if report.feasible {
                feasible.insert(projected_schedule(&solution)?, score.levels[0].value);
            }
        }
    }
    assert_eq!(unpruned_feasible.len(), 3);
    assert_eq!(unpruned_feasible, pruned_feasible);
    Ok(())
}
