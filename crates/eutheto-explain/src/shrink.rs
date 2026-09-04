use eutheto_domain_ir::{
    ConflictGroupV1, ConflictMinimality, ConflictShrinkStopReason, ConflictShrinkSummaryV1,
    ConflictUnavailableReason, DomainContractError, InfeasibilityEvidenceV1,
    MAX_EXPLANATION_RECORDS,
};
use eutheto_types::SolveBudgetView;
use std::fmt;

/// Proof strength returned by one deletion trial.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictTrialOutcome {
    /// The candidate group set was proven infeasible.
    ProvenInfeasible,
    /// The candidate group set was proven feasible.
    ProvenFeasible,
    /// The trial established neither result.
    Inconclusive,
}

/// Invalid conflict input or an impossible summary count.
#[derive(Debug)]
pub enum ConflictShrinkError {
    /// A conflict input must be non-empty, bounded, canonical, and contain valid groups.
    InvalidGroups,
    /// A domain conflict group violated its wire contract.
    InvalidGroup(DomainContractError),
    /// A platform conversion could not represent a contract count.
    CountOverflow,
}

impl fmt::Display for ConflictShrinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGroups => formatter.write_str("conflict groups are not canonical"),
            Self::InvalidGroup(error) => write!(formatter, "invalid conflict group: {error}"),
            Self::CountOverflow => formatter.write_str("conflict count cannot be represented"),
        }
    }
}

impl std::error::Error for ConflictShrinkError {}

/// Deterministically shrinks a sufficient conflict with bounded deletion trials.
///
/// The same shared budget is inspected immediately before every callback. A group is removed only
/// when the callback proves that the conflict remains infeasible without it. The first
/// inconclusive trial stops shrinking, and cancellation, expiry, and trial exhaustion retain the
/// current sufficient conflict. If the empty set is proven infeasible, the result is foundational
/// unavailability rather than an invalid empty conflict.
///
/// # Errors
/// Rejects a noncanonical input group set or an unrepresentable count.
pub fn shrink_conflict<F>(
    groups: &[ConflictGroupV1],
    max_trials: u32,
    budget: &SolveBudgetView,
    mut trial_callback: F,
) -> Result<InfeasibilityEvidenceV1, ConflictShrinkError>
where
    F: FnMut(&[ConflictGroupV1]) -> ConflictTrialOutcome,
{
    validate_groups(groups)?;
    let initial_group_count =
        u32::try_from(groups.len()).map_err(|_| ConflictShrinkError::CountOverflow)?;
    let mut remaining = groups.to_vec();
    let mut attempted_trials = 0_u32;
    let mut index = 0_usize;

    if max_trials == 0 {
        return conflict_result(
            remaining,
            initial_group_count,
            attempted_trials,
            max_trials,
            ConflictShrinkStopReason::NotAttempted,
        );
    }

    while attempted_trials < initial_group_count {
        if attempted_trials == max_trials {
            return conflict_result(
                remaining,
                initial_group_count,
                attempted_trials,
                max_trials,
                ConflictShrinkStopReason::TrialLimit,
            );
        }
        let snapshot = budget.snapshot();
        if snapshot.cancelled {
            return conflict_result(
                remaining,
                initial_group_count,
                attempted_trials,
                max_trials,
                ConflictShrinkStopReason::Cancelled,
            );
        }
        if snapshot.expired {
            return conflict_result(
                remaining,
                initial_group_count,
                attempted_trials,
                max_trials,
                ConflictShrinkStopReason::BudgetExpired,
            );
        }

        let removed = remaining.remove(index);
        attempted_trials += 1;
        let outcome = trial_callback(&remaining);
        let snapshot = budget.snapshot();
        if snapshot.cancelled {
            remaining.insert(index, removed);
            return conflict_result(
                remaining,
                initial_group_count,
                attempted_trials,
                max_trials,
                ConflictShrinkStopReason::Cancelled,
            );
        }
        if snapshot.expired {
            remaining.insert(index, removed);
            return conflict_result(
                remaining,
                initial_group_count,
                attempted_trials,
                max_trials,
                ConflictShrinkStopReason::BudgetExpired,
            );
        }
        match outcome {
            ConflictTrialOutcome::ProvenInfeasible if remaining.is_empty() => {
                return Ok(InfeasibilityEvidenceV1::Unavailable {
                    reason: ConflictUnavailableReason::FoundationalInfeasibility,
                });
            }
            ConflictTrialOutcome::ProvenInfeasible => {}
            ConflictTrialOutcome::ProvenFeasible => {
                remaining.insert(index, removed);
                index += 1;
            }
            ConflictTrialOutcome::Inconclusive => {
                remaining.insert(index, removed);
                return conflict_result(
                    remaining,
                    initial_group_count,
                    attempted_trials,
                    max_trials,
                    ConflictShrinkStopReason::Inconclusive,
                );
            }
        }
    }

    conflict_result(
        remaining,
        initial_group_count,
        attempted_trials,
        max_trials,
        ConflictShrinkStopReason::Completed,
    )
}

fn validate_groups(groups: &[ConflictGroupV1]) -> Result<(), ConflictShrinkError> {
    if groups.is_empty()
        || groups.len() > MAX_EXPLANATION_RECORDS
        || !groups.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Err(ConflictShrinkError::InvalidGroups);
    }
    for group in groups {
        group
            .validate()
            .map_err(ConflictShrinkError::InvalidGroup)?;
    }
    Ok(())
}

fn conflict_result(
    groups: Vec<ConflictGroupV1>,
    initial_group_count: u32,
    attempted_trials: u32,
    max_trials: u32,
    stop_reason: ConflictShrinkStopReason,
) -> Result<InfeasibilityEvidenceV1, ConflictShrinkError> {
    let remaining_group_count =
        u32::try_from(groups.len()).map_err(|_| ConflictShrinkError::CountOverflow)?;
    let shrink = ConflictShrinkSummaryV1 {
        initial_group_count,
        remaining_group_count,
        attempted_trials,
        max_trials,
        stop_reason,
    };
    shrink
        .validate()
        .map_err(ConflictShrinkError::InvalidGroup)?;
    let minimality = if stop_reason == ConflictShrinkStopReason::Completed {
        ConflictMinimality::ProvenMinimal
    } else {
        ConflictMinimality::Sufficient
    };
    Ok(InfeasibilityEvidenceV1::Conflict {
        groups,
        minimality,
        shrink,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use eutheto_domain_ir::AssumptionGroupId;
    use eutheto_types::{
        CancellationToken, DurationMillis, FixedMonotonicClock, MonotonicClock, ParentSolveBudget,
        RuleId,
    };
    use std::sync::Arc;
    use std::time::Duration;

    fn group(index: u8) -> Result<ConflictGroupV1, Box<dyn std::error::Error>> {
        Ok(ConflictGroupV1 {
            group_id: AssumptionGroupId::new(format!("tests.group-{index}"))?,
            required_rules: vec![
                format!("01890a5d-ac96-7b64-9f74-bbfcf30f9f{index:02x}").parse::<RuleId>()?,
            ],
        })
    }

    fn budget(
        milliseconds: u64,
        cancellation: CancellationToken,
    ) -> Result<(Arc<FixedMonotonicClock>, SolveBudgetView), Box<dyn std::error::Error>> {
        let clock = Arc::new(FixedMonotonicClock::default());
        let shared: Arc<dyn MonotonicClock> = clock.clone();
        let parent =
            ParentSolveBudget::new(DurationMillis::new(milliseconds)?, shared, cancellation)?;
        Ok((clock, parent.phase_view()))
    }

    fn conflict_parts(
        evidence: InfeasibilityEvidenceV1,
    ) -> Result<
        (
            Vec<ConflictGroupV1>,
            ConflictMinimality,
            ConflictShrinkSummaryV1,
        ),
        Box<dyn std::error::Error>,
    > {
        match evidence {
            InfeasibilityEvidenceV1::Conflict {
                groups,
                minimality,
                shrink,
            } => Ok((groups, minimality, shrink)),
            InfeasibilityEvidenceV1::Unavailable { .. } => Err("expected conflict evidence".into()),
        }
    }

    #[test]
    fn removes_only_proven_redundant_groups_and_proves_minimal()
    -> Result<(), Box<dyn std::error::Error>> {
        let groups = vec![group(1)?, group(2)?, group(3)?];
        let (_, view) = budget(1_000, CancellationToken::new())?;
        let mut trial = 0;
        let result = shrink_conflict(&groups, 3, &view, |_| {
            trial += 1;
            if trial == 1 {
                ConflictTrialOutcome::ProvenInfeasible
            } else {
                ConflictTrialOutcome::ProvenFeasible
            }
        })?;
        let (remaining, minimality, summary) = conflict_parts(result)?;
        assert_eq!(remaining, groups[1..]);
        assert_eq!(minimality, ConflictMinimality::ProvenMinimal);
        assert_eq!(summary.attempted_trials, 3);
        assert_eq!(summary.stop_reason, ConflictShrinkStopReason::Completed);
        summary.validate()?;
        Ok(())
    }

    #[test]
    fn reports_zero_trials_and_trial_exhaustion_exactly() -> Result<(), Box<dyn std::error::Error>>
    {
        let groups = vec![group(1)?, group(2)?];
        let (_, view) = budget(1_000, CancellationToken::new())?;
        let (_, minimality, zero) = conflict_parts(shrink_conflict(&groups, 0, &view, |_| {
            ConflictTrialOutcome::ProvenInfeasible
        })?)?;
        assert_eq!(minimality, ConflictMinimality::Sufficient);
        assert_eq!(zero.stop_reason, ConflictShrinkStopReason::NotAttempted);
        assert_eq!(zero.attempted_trials, 0);

        let (remaining, minimality, limited) =
            conflict_parts(shrink_conflict(&groups, 1, &view, |_| {
                ConflictTrialOutcome::ProvenInfeasible
            })?)?;
        assert_eq!(remaining, vec![groups[1].clone()]);
        assert_eq!(minimality, ConflictMinimality::Sufficient);
        assert_eq!(limited.stop_reason, ConflictShrinkStopReason::TrialLimit);
        assert_eq!(limited.attempted_trials, 1);
        limited.validate()?;
        Ok(())
    }

    #[test]
    fn distinguishes_shared_budget_expiry_cancellation_and_inconclusive()
    -> Result<(), Box<dyn std::error::Error>> {
        let groups = vec![group(1)?, group(2)?];
        let (clock, expired_view) = budget(5, CancellationToken::new())?;
        clock.advance(Duration::from_millis(5))?;
        let expired_callback_called = std::cell::Cell::new(false);
        let (_, _, expired) = conflict_parts(shrink_conflict(&groups, 2, &expired_view, |_| {
            expired_callback_called.set(true);
            ConflictTrialOutcome::Inconclusive
        })?)?;
        assert_eq!(expired.stop_reason, ConflictShrinkStopReason::BudgetExpired);
        assert_eq!(expired.attempted_trials, 0);
        assert!(!expired_callback_called.get());

        let cancellation = CancellationToken::new();
        let (_, cancelled_view) = budget(1_000, cancellation.clone())?;
        cancellation.cancel();
        let cancelled_callback_called = std::cell::Cell::new(false);
        let (_, _, cancelled) =
            conflict_parts(shrink_conflict(&groups, 2, &cancelled_view, |_| {
                cancelled_callback_called.set(true);
                ConflictTrialOutcome::Inconclusive
            })?)?;
        assert_eq!(cancelled.stop_reason, ConflictShrinkStopReason::Cancelled);
        assert_eq!(cancelled.attempted_trials, 0);
        assert!(!cancelled_callback_called.get());

        let (_, view) = budget(1_000, CancellationToken::new())?;
        let (remaining, minimality, inconclusive) =
            conflict_parts(shrink_conflict(&groups, 2, &view, |_| {
                ConflictTrialOutcome::Inconclusive
            })?)?;
        assert_eq!(remaining, groups);
        assert_eq!(minimality, ConflictMinimality::Sufficient);
        assert_eq!(
            inconclusive.stop_reason,
            ConflictShrinkStopReason::Inconclusive
        );
        assert_eq!(inconclusive.attempted_trials, 1);
        Ok(())
    }

    #[test]
    fn converts_a_proven_empty_conflict_to_foundational_unavailability()
    -> Result<(), Box<dyn std::error::Error>> {
        let groups = vec![group(1)?];
        let (_, view) = budget(1_000, CancellationToken::new())?;
        assert_eq!(
            shrink_conflict(&groups, 1, &view, |_| {
                ConflictTrialOutcome::ProvenInfeasible
            },)?,
            InfeasibilityEvidenceV1::Unavailable {
                reason: ConflictUnavailableReason::FoundationalInfeasibility,
            }
        );
        Ok(())
    }
}
