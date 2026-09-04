use eutheto_domain_ir::{
    AcceptedResult, AcceptedResultRefV1, AssignmentComparisonV1, AssignmentLockStateV1,
    ComparisonBindingV1, ComparisonOrdering, DomainContractError, ExplanationCertainty,
    LockComparisonV1, MetricComparisonV1, RuleComparisonV1, RunComparisonSideV1, RunComparisonV1,
    RunManifestV1, RunTerminalOutcomeV1, ScoreCategoryComparisonV1, ScoreLevelComparisonV1,
    SolutionComparisonV1,
};
use eutheto_types::SolveStatus;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt;

/// Identifies one side when reporting an invalid supplied contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonSide {
    /// The base accepted result or manifest.
    Base,
    /// The candidate accepted result or manifest.
    Candidate,
}

/// One caller-supplied lock-state pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComparisonLockPair {
    /// Assignment to which both observations apply.
    pub assignment_id: eutheto_domain_ir::DomainAssignmentId,
    /// Base lock state.
    pub before: AssignmentLockStateV1,
    /// Candidate lock state.
    pub after: AssignmentLockStateV1,
}

/// Both terminal manifests supplied as one inseparable pair.
#[derive(Clone, Copy, Debug)]
pub struct ComparisonRunManifests<'a> {
    /// Manifest associated with the base accepted result.
    pub base: &'a RunManifestV1,
    /// Manifest associated with the candidate accepted result.
    pub candidate: &'a RunManifestV1,
}

/// Optional, nonserializable comparison authority supplied by the caller.
#[derive(Clone, Copy, Debug, Default)]
pub struct ComparisonContext<'a> {
    /// Canonical assignment-ID-sorted lock pairs.
    pub locks: &'a [ComparisonLockPair],
    /// Either both validated manifests or neither.
    pub manifests: Option<ComparisonRunManifests<'a>>,
}

/// Failure to compare two independently accepted results.
#[derive(Debug)]
pub enum ComparisonError {
    /// An accepted result failed its own contract.
    InvalidAcceptedResult {
        /// Invalid side.
        side: ComparisonSide,
        /// Underlying domain-contract failure.
        source: DomainContractError,
    },
    /// The scenario, pack, projection, same-revision scope, or score shape is incompatible.
    Incompatible,
    /// Lock pairs are not strictly ordered by assignment ID.
    NonCanonicalLocks,
    /// A supplied manifest failed validation or did not bind its accepted result.
    InvalidManifest {
        /// Invalid side.
        side: ComparisonSide,
        /// Underlying domain-contract failure, when manifest validation itself failed.
        source: Option<DomainContractError>,
    },
    /// An exact score or category delta overflowed `i64`.
    ArithmeticOverflow,
    /// The resulting strict domain comparison contract was rejected.
    InvalidComparison(DomainContractError),
}

impl fmt::Display for ComparisonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAcceptedResult { side, .. } => {
                write!(formatter, "invalid {side:?} accepted result")
            }
            Self::Incompatible => formatter.write_str("accepted results are incompatible"),
            Self::NonCanonicalLocks => {
                formatter.write_str("comparison lock pairs are not canonical")
            }
            Self::InvalidManifest { side, .. } => {
                write!(formatter, "invalid {side:?} run manifest")
            }
            Self::ArithmeticOverflow => formatter.write_str("score comparison arithmetic overflow"),
            Self::InvalidComparison(error) => {
                write!(formatter, "invalid solution comparison: {error}")
            }
        }
    }
}

impl std::error::Error for ComparisonError {}

/// Compares two independently accepted results in linear time over canonical assignments/rules.
///
/// Cross-revision comparisons are supported. Both revisions, document hashes, verification scopes,
/// and accepted-result checksums remain bound into the result. Lock and terminal status evidence is
/// emitted only from explicit context; accepted results alone never fabricate either.
///
/// # Errors
/// Rejects invalid inputs, incompatible bindings or score shapes, noncanonical context, manifest
/// mismatches, and checked-arithmetic overflow.
pub fn compare_accepted_results(
    base: &AcceptedResult,
    candidate: &AcceptedResult,
    context: Option<&ComparisonContext<'_>>,
) -> Result<SolutionComparisonV1, ComparisonError> {
    base.validate()
        .map_err(|source| ComparisonError::InvalidAcceptedResult {
            side: ComparisonSide::Base,
            source,
        })?;
    candidate
        .validate()
        .map_err(|source| ComparisonError::InvalidAcceptedResult {
            side: ComparisonSide::Candidate,
            source,
        })?;
    validate_compatibility(base, candidate)?;

    let (assignments, mut affected_entities) = assignment_deltas(base, candidate);
    let (rules, rule_entities) = rule_deltas(base, candidate);
    affected_entities.extend(rule_entities);
    let score_levels = score_deltas(base, candidate)?;
    let metrics = metric_deltas(base, candidate);
    let locks = lock_deltas(context)?;
    let runs = run_comparison(base, candidate, context)?;
    let ordering = ordering(base, candidate)?;

    SolutionComparisonV1::new(
        binding(base),
        binding(candidate),
        base.verification.score.clone(),
        candidate.verification.score.clone(),
        assignments,
        rules,
        score_levels,
        metrics,
        locks,
        runs,
        affected_entities.into_iter().collect(),
        ordering,
    )
    .map_err(ComparisonError::InvalidComparison)
}

fn validate_compatibility(
    base: &AcceptedResult,
    candidate: &AcceptedResult,
) -> Result<(), ComparisonError> {
    if base.solution.pack_id != candidate.solution.pack_id
        || base.solution.scenario_id != candidate.solution.scenario_id
        || base.solution.projection_version != candidate.solution.projection_version
        || (base.solution.scenario_revision == candidate.solution.scenario_revision
            && base.verification.verification_scope_checksum
                != candidate.verification.verification_scope_checksum)
        || base
            .verification
            .score
            .compare(&candidate.verification.score)
            .is_err()
    {
        return Err(ComparisonError::Incompatible);
    }
    Ok(())
}

fn binding(result: &AcceptedResult) -> ComparisonBindingV1 {
    ComparisonBindingV1 {
        pack_id: result.solution.pack_id.clone(),
        scenario_id: result.solution.scenario_id,
        scenario_revision: result.solution.scenario_revision,
        document_hash: result.verification.document_hash.clone(),
        projection_version: result.solution.projection_version,
        verification_scope_checksum: result.verification.verification_scope_checksum.clone(),
        accepted_result: AcceptedResultRefV1 {
            solution_id: result.solution.solution_id,
            result_checksum: result.checksum.clone(),
        },
    }
}

fn assignment_deltas(
    base: &AcceptedResult,
    candidate: &AcceptedResult,
) -> (
    Vec<AssignmentComparisonV1>,
    BTreeSet<eutheto_domain_ir::DomainEntityRef>,
) {
    let before = &base.solution.assignments;
    let after = &candidate.solution.assignments;
    let mut left = 0;
    let mut right = 0;
    let mut deltas = Vec::new();
    let mut entities = BTreeSet::new();
    while left < before.len() || right < after.len() {
        match (before.get(left), after.get(right)) {
            (Some(old), Some(new)) => match old.id.cmp(&new.id) {
                Ordering::Less => {
                    entities.insert(old.entity.clone());
                    deltas.push(AssignmentComparisonV1::Removed {
                        before: old.clone(),
                    });
                    left += 1;
                }
                Ordering::Greater => {
                    entities.insert(new.entity.clone());
                    deltas.push(AssignmentComparisonV1::Added { after: new.clone() });
                    right += 1;
                }
                Ordering::Equal => {
                    if old != new {
                        entities.insert(old.entity.clone());
                        entities.insert(new.entity.clone());
                        deltas.push(AssignmentComparisonV1::Changed {
                            before: old.clone(),
                            after: new.clone(),
                        });
                    }
                    left += 1;
                    right += 1;
                }
            },
            (Some(old), None) => {
                entities.insert(old.entity.clone());
                deltas.push(AssignmentComparisonV1::Removed {
                    before: old.clone(),
                });
                left += 1;
            }
            (None, Some(new)) => {
                entities.insert(new.entity.clone());
                deltas.push(AssignmentComparisonV1::Added { after: new.clone() });
                right += 1;
            }
            (None, None) => break,
        }
    }
    (deltas, entities)
}

fn rule_deltas(
    base: &AcceptedResult,
    candidate: &AcceptedResult,
) -> (
    Vec<RuleComparisonV1>,
    BTreeSet<eutheto_domain_ir::DomainEntityRef>,
) {
    let before = &base.verification.required_rule_results;
    let after = &candidate.verification.required_rule_results;
    let mut left = 0;
    let mut right = 0;
    let mut deltas = Vec::new();
    let mut entities = BTreeSet::new();
    while left < before.len() || right < after.len() {
        let (rule_id, old, new) = match (before.get(left), after.get(right)) {
            (Some(old), Some(new)) => match old.rule_id.cmp(&new.rule_id) {
                Ordering::Less => {
                    left += 1;
                    (old.rule_id, Some(old), None)
                }
                Ordering::Greater => {
                    right += 1;
                    (new.rule_id, None, Some(new))
                }
                Ordering::Equal => {
                    left += 1;
                    right += 1;
                    if old == new {
                        continue;
                    }
                    (old.rule_id, Some(old), Some(new))
                }
            },
            (Some(old), None) => {
                left += 1;
                (old.rule_id, Some(old), None)
            }
            (None, Some(new)) => {
                right += 1;
                (new.rule_id, None, Some(new))
            }
            (None, None) => break,
        };
        for entity in old
            .into_iter()
            .flat_map(|value| &value.affected_entities)
            .chain(new.into_iter().flat_map(|value| &value.affected_entities))
        {
            entities.insert(entity.clone());
        }
        deltas.push(RuleComparisonV1 {
            rule_id,
            before: old.cloned(),
            after: new.cloned(),
        });
    }
    (deltas, entities)
}

fn score_deltas(
    base: &AcceptedResult,
    candidate: &AcceptedResult,
) -> Result<Vec<ScoreLevelComparisonV1>, ComparisonError> {
    base.verification
        .score
        .levels
        .iter()
        .zip(&candidate.verification.score.levels)
        .map(|(before, after)| {
            let delta = after
                .value
                .checked_sub(before.value)
                .ok_or(ComparisonError::ArithmeticOverflow)?;
            let mut categories = Vec::with_capacity(
                before.category_breakdown.len() + after.category_breakdown.len(),
            );
            let mut left = before.category_breakdown.iter().peekable();
            let mut right = after.category_breakdown.iter().peekable();
            loop {
                let (category_id, old, new) = match (left.peek(), right.peek()) {
                    (Some((old_id, old)), Some((new_id, new))) => match old_id.cmp(new_id) {
                        Ordering::Less => {
                            let value = ((**old_id).clone(), Some(**old), None);
                            left.next();
                            value
                        }
                        Ordering::Equal => {
                            let value = ((**old_id).clone(), Some(**old), Some(**new));
                            left.next();
                            right.next();
                            value
                        }
                        Ordering::Greater => {
                            let value = ((**new_id).clone(), None, Some(**new));
                            right.next();
                            value
                        }
                    },
                    (Some((old_id, old)), None) => {
                        let value = ((**old_id).clone(), Some(**old), None);
                        left.next();
                        value
                    }
                    (None, Some((new_id, new))) => {
                        let value = ((**new_id).clone(), None, Some(**new));
                        right.next();
                        value
                    }
                    (None, None) => break,
                };
                let category_delta = match (old, new) {
                    (Some(old), Some(new)) => Some(
                        new.checked_sub(old)
                            .ok_or(ComparisonError::ArithmeticOverflow)?,
                    ),
                    _ => None,
                };
                categories.push(ScoreCategoryComparisonV1 {
                    category_id,
                    before: old,
                    after: new,
                    delta: category_delta,
                });
            }
            Ok(ScoreLevelComparisonV1 {
                level_id: before.level_id.clone(),
                direction: before.direction,
                before: before.value,
                after: after.value,
                delta,
                categories,
            })
        })
        .collect()
}

fn metric_deltas(base: &AcceptedResult, candidate: &AcceptedResult) -> Vec<MetricComparisonV1> {
    let mut before = base.verification.metrics.iter().peekable();
    let mut after = candidate.verification.metrics.iter().peekable();
    let mut deltas = Vec::new();
    loop {
        let (metric_id, old, new) = match (before.peek(), after.peek()) {
            (Some((old_id, old)), Some((new_id, new))) => match old_id.cmp(new_id) {
                Ordering::Less => {
                    let value = ((**old_id).clone(), Some((**old).clone()), None);
                    before.next();
                    value
                }
                Ordering::Equal => {
                    let value = (
                        (**old_id).clone(),
                        Some((**old).clone()),
                        Some((**new).clone()),
                    );
                    before.next();
                    after.next();
                    value
                }
                Ordering::Greater => {
                    let value = ((**new_id).clone(), None, Some((**new).clone()));
                    after.next();
                    value
                }
            },
            (Some((old_id, old)), None) => {
                let value = ((**old_id).clone(), Some((**old).clone()), None);
                before.next();
                value
            }
            (None, Some((new_id, new))) => {
                let value = ((**new_id).clone(), None, Some((**new).clone()));
                after.next();
                value
            }
            (None, None) => break,
        };
        if old != new {
            deltas.push(MetricComparisonV1 {
                metric_id,
                before: old,
                after: new,
            });
        }
    }
    deltas
}

fn lock_deltas(
    context: Option<&ComparisonContext<'_>>,
) -> Result<Vec<LockComparisonV1>, ComparisonError> {
    let Some(context) = context else {
        return Ok(Vec::new());
    };
    if !context
        .locks
        .windows(2)
        .all(|pair| pair[0].assignment_id < pair[1].assignment_id)
    {
        return Err(ComparisonError::NonCanonicalLocks);
    }
    Ok(context
        .locks
        .iter()
        .map(|pair| LockComparisonV1 {
            assignment_id: pair.assignment_id.clone(),
            before: pair.before.clone(),
            after: pair.after.clone(),
            preserved: pair.before == pair.after,
        })
        .collect())
}

fn run_comparison(
    base: &AcceptedResult,
    candidate: &AcceptedResult,
    context: Option<&ComparisonContext<'_>>,
) -> Result<Option<RunComparisonV1>, ComparisonError> {
    let Some(manifests) = context.and_then(|value| value.manifests) else {
        return Ok(None);
    };
    validate_manifest(manifests.base, base, ComparisonSide::Base)?;
    validate_manifest(manifests.candidate, candidate, ComparisonSide::Candidate)?;
    Ok(Some(RunComparisonV1 {
        base: run_side(manifests.base),
        candidate: run_side(manifests.candidate),
    }))
}

fn validate_manifest(
    manifest: &RunManifestV1,
    accepted: &AcceptedResult,
    side: ComparisonSide,
) -> Result<(), ComparisonError> {
    manifest
        .validate()
        .map_err(|source| ComparisonError::InvalidManifest {
            side,
            source: Some(source),
        })?;
    let expected = &accepted.solution;
    match &manifest.outcome {
        RunTerminalOutcomeV1::Accepted {
            solution_id,
            accepted_result_checksum,
            verification_checksum,
            ..
        } if solution_id == &expected.solution_id
            && accepted_result_checksum == &accepted.checksum
            && verification_checksum == &accepted.verification.checksum =>
        {
            Ok(())
        }
        _ => Err(ComparisonError::InvalidManifest { side, source: None }),
    }
}

fn run_side(manifest: &RunManifestV1) -> RunComparisonSideV1 {
    RunComparisonSideV1 {
        run_id: manifest.run_id,
        run_manifest_checksum: manifest.checksum.clone(),
        outcome: manifest.outcome.clone(),
        certainty: terminal_certainty(&manifest.outcome),
    }
}

fn terminal_certainty(outcome: &RunTerminalOutcomeV1) -> ExplanationCertainty {
    match outcome {
        RunTerminalOutcomeV1::Accepted {
            status: SolveStatus::Optimal,
            ..
        }
        | RunTerminalOutcomeV1::NoResult {
            status: SolveStatus::Infeasible | SolveStatus::Unbounded,
        } => ExplanationCertainty::BackendProof,
        RunTerminalOutcomeV1::Accepted {
            status: SolveStatus::Feasible,
            ..
        } => ExplanationCertainty::IndependentlyVerified,
        _ => ExplanationCertainty::Deterministic,
    }
}

fn ordering(
    base: &AcceptedResult,
    candidate: &AcceptedResult,
) -> Result<ComparisonOrdering, ComparisonError> {
    match base
        .verification
        .score
        .compare(&candidate.verification.score)
        .map_err(|_| ComparisonError::Incompatible)?
    {
        Ordering::Less => Ok(ComparisonOrdering::Worse),
        Ordering::Equal => Ok(ComparisonOrdering::Equivalent),
        Ordering::Greater => Ok(ComparisonOrdering::Better),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eutheto_domain_ir::{
        DomainAssignment, DomainAssignmentId, DomainEntityId, DomainEntityKindId, DomainEntityRef,
        MetricId, MetricValue, NORMALIZED_SOLUTION_SCHEMA_VERSION, NormalizedSolution,
        OptimizationDirection, RuleEvaluation, RunPhaseTimingsV1, ScoreCategoryId, ScoreLevelId,
        ScoreLevelValue, ScoreVector, VerificationContextV1, VerificationReport, blake3_hex,
    };
    use eutheto_types::{
        DurationMillis, PackId, Rfc3339Timestamp, RuleId, ScenarioId, SolutionId, SolveRunId,
    };
    use std::collections::BTreeMap;

    fn hash(label: &str) -> String {
        blake3_hex(label.as_bytes())
    }

    fn entity(name: &str) -> Result<DomainEntityRef, Box<dyn std::error::Error>> {
        Ok(DomainEntityRef {
            kind: DomainEntityKindId::new("tests.person")?,
            id: DomainEntityId::new(format!("tests.{name}"))?,
        })
    }

    fn assignment(name: &str, value: i64) -> Result<DomainAssignment, Box<dyn std::error::Error>> {
        Ok(DomainAssignment {
            id: DomainAssignmentId::new(format!("tests.{name}"))?,
            entity: entity(name)?,
            value: eutheto_domain_ir::AssignmentValue::Integer(value),
            evidence: Vec::new(),
        })
    }

    fn rule(
        suffix: u8,
        entity_name: &str,
        message: &str,
    ) -> Result<RuleEvaluation, Box<dyn std::error::Error>> {
        Ok(RuleEvaluation {
            rule_id: format!("01890a5d-ac96-7b64-9f74-bbfcf30f9f{suffix:02x}").parse::<RuleId>()?,
            satisfied: true,
            affected_entities: vec![entity(entity_name)?],
            message_key: message.to_owned(),
            expected: BTreeMap::new(),
            observed: BTreeMap::new(),
            evidence: Vec::new(),
        })
    }

    struct AcceptedArgs<'a> {
        revision: u64,
        solution_suffix: u8,
        score: i64,
        assignments: Vec<DomainAssignment>,
        rules: Vec<RuleEvaluation>,
        category: Option<i64>,
        metrics: BTreeMap<MetricId, MetricValue>,
        scenario: &'a str,
        pack: &'a str,
        projection_version: u32,
    }

    fn accepted(args: AcceptedArgs<'_>) -> Result<AcceptedResult, Box<dyn std::error::Error>> {
        let scenario_id = args.scenario.parse::<ScenarioId>()?;
        let mut solution = NormalizedSolution {
            schema_version: NORMALIZED_SOLUTION_SCHEMA_VERSION,
            pack_id: PackId::new(args.pack)?,
            scenario_id,
            scenario_revision: args.revision,
            projection_version: args.projection_version,
            solution_id: format!(
                "01890a5d-ac96-7b64-9f74-bbfcf30f9f{:02x}",
                args.solution_suffix
            )
            .parse::<SolutionId>()?,
            assignments: args.assignments,
        };
        solution.canonicalize()?;
        let mut category_breakdown = BTreeMap::new();
        if let Some(value) = args.category {
            category_breakdown.insert(ScoreCategoryId::new("tests.category")?, value);
        }
        let score = ScoreVector {
            feasibility: 0,
            levels: vec![ScoreLevelValue {
                level_id: ScoreLevelId::new("tests.primary")?,
                value: args.score,
                direction: OptimizationDirection::Minimize,
                category_breakdown,
            }],
        };
        let context = VerificationContextV1::new(
            scenario_id,
            args.revision,
            hash(&format!("document-{}", args.revision)),
            hash(&format!("model-{}", args.revision)),
            solution.canonical_hash()?,
            hash(&format!("scope-{}", args.revision)),
        )?;
        let report =
            VerificationReport::new(&context, args.rules, score, Vec::new(), args.metrics)?;
        Ok(AcceptedResult::new(solution, report)?)
    }

    fn pair(
        base_score: i64,
        candidate_score: i64,
    ) -> Result<(AcceptedResult, AcceptedResult), Box<dyn std::error::Error>> {
        let scenario = "01890a5d-ac96-7b64-9f74-bbfcf30f9f80";
        let base = accepted(AcceptedArgs {
            revision: 1,
            solution_suffix: 0x10,
            score: base_score,
            assignments: vec![assignment("a", 1)?, assignment("b", 2)?],
            rules: vec![rule(1, "a", "tests.rule.base")?],
            category: Some(base_score),
            metrics: BTreeMap::from([
                (MetricId::new("tests.changed")?, MetricValue::Integer(1)),
                (MetricId::new("tests.removed")?, MetricValue::Integer(7)),
            ]),
            scenario,
            pack: "official.test",
            projection_version: 1,
        })?;
        let candidate = accepted(AcceptedArgs {
            revision: 2,
            solution_suffix: 0x11,
            score: candidate_score,
            assignments: vec![assignment("b", 3)?, assignment("c", 4)?],
            rules: vec![
                rule(1, "b", "tests.rule.changed")?,
                rule(2, "c", "tests.rule.added")?,
            ],
            category: None,
            metrics: BTreeMap::from([
                (MetricId::new("tests.added")?, MetricValue::Integer(8)),
                (MetricId::new("tests.changed")?, MetricValue::Integer(2)),
            ]),
            scenario,
            pack: "official.test",
            projection_version: 1,
        })?;
        Ok((base, candidate))
    }

    fn manifest(
        suffix: u8,
        accepted: &AcceptedResult,
        status: SolveStatus,
    ) -> Result<RunManifestV1, Box<dyn std::error::Error>> {
        let run_id =
            format!("01890a5d-ac96-7b64-9f74-bbfcf30f9f{suffix:02x}").parse::<SolveRunId>()?;
        Ok(RunManifestV1::new(
            run_id,
            hash(&format!("input-{suffix}")),
            RunTerminalOutcomeV1::Accepted {
                status,
                solution_id: accepted.solution.solution_id,
                accepted_result_checksum: accepted.checksum.clone(),
                verification_checksum: accepted.verification.checksum.clone(),
            },
            "2026-09-03T12:00:00Z".parse::<Rfc3339Timestamp>()?,
            "2026-09-03T12:00:01Z".parse::<Rfc3339Timestamp>()?,
            Some(DurationMillis::new(1_000)?),
            Some(DurationMillis::new(100)?),
            Some(DurationMillis::new(200)?),
            RunPhaseTimingsV1::default(),
            Vec::new(),
        )?)
    }

    #[test]
    fn compares_cross_revision_changes_and_absent_values() -> Result<(), Box<dyn std::error::Error>>
    {
        let (base, candidate) = pair(10, 5)?;
        let comparison = compare_accepted_results(&base, &candidate, None)?;
        assert_eq!(comparison.base.scenario_revision, 1);
        assert_eq!(comparison.candidate.scenario_revision, 2);
        assert_eq!(comparison.ordering, ComparisonOrdering::Better);
        assert_eq!(comparison.assignments.len(), 3);
        assert_eq!(comparison.rules.len(), 2);
        assert_eq!(comparison.score_levels[0].delta, -5);
        assert_eq!(comparison.score_levels[0].categories[0].before, Some(10));
        assert_eq!(comparison.score_levels[0].categories[0].after, None);
        assert_eq!(comparison.metrics.len(), 3);
        assert!(comparison.runs.is_none());
        assert!(comparison.locks.is_empty());
        assert_eq!(
            comparison.affected_entities,
            vec![entity("a")?, entity("b")?, entity("c")?]
        );
        comparison.validate()?;
        Ok(())
    }

    #[test]
    fn emits_locks_and_status_only_from_bound_context() -> Result<(), Box<dyn std::error::Error>> {
        let (base, candidate) = pair(10, 5)?;
        let base_manifest = manifest(0x20, &base, SolveStatus::Feasible)?;
        let candidate_manifest = manifest(0x21, &candidate, SolveStatus::Optimal)?;
        let locks = [ComparisonLockPair {
            assignment_id: DomainAssignmentId::new("tests.b")?,
            before: AssignmentLockStateV1::Locked {
                value: eutheto_domain_ir::AssignmentValue::Integer(2),
            },
            after: AssignmentLockStateV1::Locked {
                value: eutheto_domain_ir::AssignmentValue::Integer(3),
            },
        }];
        let context = ComparisonContext {
            locks: &locks,
            manifests: Some(ComparisonRunManifests {
                base: &base_manifest,
                candidate: &candidate_manifest,
            }),
        };
        let comparison = compare_accepted_results(&base, &candidate, Some(&context))?;
        assert!(!comparison.locks[0].preserved);
        let runs = comparison.runs.ok_or("missing run comparison")?;
        assert_eq!(runs.base.run_manifest_checksum, base_manifest.checksum);
        assert_eq!(
            runs.candidate.run_manifest_checksum,
            candidate_manifest.checksum
        );
        assert_eq!(
            runs.base.certainty,
            ExplanationCertainty::IndependentlyVerified
        );
        assert_eq!(runs.candidate.certainty, ExplanationCertainty::BackendProof);
        Ok(())
    }

    #[test]
    fn rejects_delta_overflow_and_incompatible_bindings() -> Result<(), Box<dyn std::error::Error>>
    {
        let (minimum, maximum) = pair(i64::MIN, i64::MAX)?;
        assert!(matches!(
            compare_accepted_results(&minimum, &maximum, None),
            Err(ComparisonError::ArithmeticOverflow)
        ));

        let scenario = "01890a5d-ac96-7b64-9f74-bbfcf30f9f81";
        let incompatible = accepted(AcceptedArgs {
            revision: 2,
            solution_suffix: 0x12,
            score: 5,
            assignments: Vec::new(),
            rules: Vec::new(),
            category: None,
            metrics: BTreeMap::new(),
            scenario,
            pack: "official.other",
            projection_version: 2,
        })?;
        assert!(matches!(
            compare_accepted_results(&minimum, &incompatible, None),
            Err(ComparisonError::Incompatible)
        ));
        Ok(())
    }
}
