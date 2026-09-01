//! Version-1 solver-neutral mathematical model and shape-safe constructors.

use crate::ids::{
    AssumptionId, BoolVariableId, CompilerId, ConstraintTag, IntVariableId, IntervalVariableId,
    MetadataKey, ObjectiveLevelId, ObjectiveTermId, PlanningConstraintId, ProjectionId,
    ProvenanceId,
};
use eutheto_domain_ir::{
    AssignmentValue, DomainAssignmentId, DomainEntityRef, OptimizationDirection, ScoreCategoryId,
};
use eutheto_types::{PackId, ScenarioId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Planning IR schema version supported by this crate.
pub const PLANNING_IR_SCHEMA_VERSION: u32 = 1;
/// Projection schema version supported by this crate.
pub const PROJECTION_SCHEMA_VERSION: u32 = 1;

/// Inclusive integer range.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InclusiveRange {
    /// Inclusive minimum.
    pub start: i64,
    /// Inclusive maximum.
    pub end: i64,
}

/// Non-empty union of sorted, pairwise nonadjacent inclusive ranges.
///
/// Construction sorts, rejects reversed ranges, and merges overlapping or adjacent ranges.
/// Validation later rejects a serialized domain that did not use this exact representation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntDomain {
    /// Canonical ranges.
    pub inclusive_ranges: Vec<InclusiveRange>,
}

impl IntDomain {
    /// Normalizes ranges into their exact canonical union.
    ///
    /// # Errors
    /// Rejects empty input, reversed bounds, and adjacency arithmetic overflow.
    pub fn new(mut ranges: Vec<InclusiveRange>) -> Result<Self, ModelError> {
        if ranges.is_empty() {
            return Err(ModelError::EmptyDomain);
        }
        if ranges.iter().any(|range| range.start > range.end) {
            return Err(ModelError::ReversedRange);
        }
        ranges.sort();
        let mut normalized: Vec<InclusiveRange> = Vec::with_capacity(ranges.len());
        for range in ranges {
            if let Some(last) = normalized.last_mut() {
                let touches = range.start <= last.end
                    || last
                        .end
                        .checked_add(1)
                        .is_some_and(|next| range.start <= next);
                if touches {
                    last.end = last.end.max(range.end);
                    continue;
                }
            }
            normalized.push(range);
        }
        Ok(Self {
            inclusive_ranges: normalized,
        })
    }

    /// Returns whether `value` belongs to the represented union.
    #[must_use]
    pub fn contains(&self, value: i64) -> bool {
        self.inclusive_ranges
            .iter()
            .any(|range| range.start <= value && value <= range.end)
    }

    /// Returns finite minimum and maximum.
    #[must_use]
    pub fn bounds(&self) -> Option<(i64, i64)> {
        Some((
            self.inclusive_ranges.first()?.start,
            self.inclusive_ranges.last()?.end,
        ))
    }
}

/// A Boolean literal. `positive == true` means the variable; false means its negation.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Literal {
    /// Referenced Boolean variable.
    pub variable: BoolVariableId,
    /// Literal polarity.
    pub positive: bool,
}

impl Literal {
    /// Creates a positive literal.
    #[must_use]
    pub const fn positive(variable: BoolVariableId) -> Self {
        Self {
            variable,
            positive: true,
        }
    }

    /// Creates a negative literal.
    #[must_use]
    pub const fn negative(variable: BoolVariableId) -> Self {
        Self {
            variable,
            positive: false,
        }
    }

    /// Returns the opposite polarity.
    #[must_use]
    pub fn negated(&self) -> Self {
        Self {
            variable: self.variable.clone(),
            positive: !self.positive,
        }
    }
}

/// Boolean variable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoolVariable {
    /// Stable identity.
    pub id: BoolVariableId,
    /// Required provenance.
    pub provenance: ProvenanceId,
}

/// Bounded integer variable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntVariable {
    /// Stable identity.
    pub id: IntVariableId,
    /// Exact finite domain.
    pub domain: IntDomain,
    /// Required provenance.
    pub provenance: ProvenanceId,
}

/// Half-open interval variable `[start, end)`.
///
/// When `presence` is `None`, it is mandatory. When present, a true literal enforces
/// non-negative duration and `start + duration == end`; a false literal leaves all three
/// component integers unconstrained and global interval constraints ignore the interval.
/// Zero duration occupies no integer instant.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntervalVariable {
    /// Stable identity.
    pub id: IntervalVariableId,
    /// Start component.
    pub start: IntVariableId,
    /// Duration component.
    pub duration: IntVariableId,
    /// End component.
    pub end: IntVariableId,
    /// Optional presence condition.
    pub presence: Option<Literal>,
    /// Required provenance.
    pub provenance: ProvenanceId,
}

/// Planning variable. Integer components of intervals are ordinary declared integer variables.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "type",
    content = "value",
    deny_unknown_fields
)]
pub enum Variable {
    /// Boolean variable.
    Boolean(BoolVariable),
    /// Integer variable.
    Integer(IntVariable),
    /// Interval variable.
    Interval(IntervalVariable),
}

impl Variable {
    /// Stable ID string used for canonical root ordering.
    #[must_use]
    pub fn canonical_id(&self) -> &str {
        match self {
            Self::Boolean(value) => value.id.as_str(),
            Self::Integer(value) => value.id.as_str(),
            Self::Interval(value) => value.id.as_str(),
        }
    }

    /// Required provenance reference.
    #[must_use]
    pub const fn provenance(&self) -> &ProvenanceId {
        match self {
            Self::Boolean(value) => &value.provenance,
            Self::Integer(value) => &value.provenance,
            Self::Interval(value) => &value.provenance,
        }
    }
}

/// One integer linear term.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinearTerm {
    /// Variable.
    pub variable: IntVariableId,
    /// Nonzero coefficient.
    pub coefficient: i64,
}

/// Canonical integer linear expression `constant + sum(coefficient * variable)`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinearExpression {
    /// Terms strictly ordered by variable with no zero coefficient.
    pub terms: Vec<LinearTerm>,
    /// Constant offset.
    pub constant: i64,
}

impl LinearExpression {
    /// Combines duplicate terms, removes zero terms, and sorts by variable.
    ///
    /// # Errors
    /// Returns [`ModelError::ArithmeticOverflow`] if coefficient combination overflows.
    pub fn new(terms: Vec<LinearTerm>, constant: i64) -> Result<Self, ModelError> {
        let mut combined: BTreeMap<IntVariableId, i64> = BTreeMap::new();
        for term in terms {
            let prior = combined.get(&term.variable).copied().unwrap_or(0);
            let coefficient = prior
                .checked_add(term.coefficient)
                .ok_or(ModelError::ArithmeticOverflow)?;
            combined.insert(term.variable, coefficient);
        }
        let terms = combined
            .into_iter()
            .filter_map(|(variable, coefficient)| {
                (coefficient != 0).then_some(LinearTerm {
                    variable,
                    coefficient,
                })
            })
            .collect();
        Ok(Self { terms, constant })
    }
}

/// Integer comparison operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ComparisonOp {
    /// Equal.
    Equal,
    /// Less than or equal.
    LessOrEqual,
    /// Greater than or equal.
    GreaterOrEqual,
}

/// Linear comparison against a constant right-hand side.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinearComparison {
    /// Left expression.
    pub expression: LinearExpression,
    /// Operation.
    pub op: ComparisonOp,
    /// Right constant.
    pub rhs: i64,
}

/// Version-1 primitive vocabulary. Circuit/path is deliberately absent.
///
/// Empty semantics are exact: `BoolOr` is false; `BoolAnd` is true;
/// `AtMostOne` is true for zero/one literal; `ExactlyOne` is false when empty;
/// cardinality requires `0 <= min <= max <= len` and only `0..=0` is valid when empty;
/// `AllDifferent` is true for zero/one variable. Min/max require nonempty inputs.
/// Allowed-table with no rows is false; zero arity with exactly one empty row is true.
/// Forbidden-table with no rows is true; zero arity with one empty row is false.
/// Table rows have exact arity. Element is false when its assigned index is outside
/// `0..values.len()`; validation does not require the complete index domain to be in range.
/// Reification is iff. Enforcement on the enclosing record is a conjunction; empty means
/// always active.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "type",
    content = "value",
    deny_unknown_fields
)]
pub enum Constraint {
    /// Disjunction, false when empty.
    BoolOr { literals: Vec<Literal> },
    /// Conjunction, true when empty.
    BoolAnd { literals: Vec<Literal> },
    /// `antecedent -> consequent`.
    Implication {
        antecedent: Literal,
        consequent: Literal,
    },
    /// Boolean iff.
    Equivalence { left: Literal, right: Literal },
    /// At most one literal is true.
    AtMostOne { literals: Vec<Literal> },
    /// Exactly one literal is true; empty is false.
    ExactlyOne { literals: Vec<Literal> },
    /// Inclusive number of true literals.
    CardinalityRange {
        literals: Vec<Literal>,
        min: u64,
        max: u64,
    },
    /// Integer linear equality/inequality.
    LinearComparison(LinearComparison),
    /// Literal is true iff the complete linear comparison is true.
    ReifiedLinearComparison {
        literal: Literal,
        comparison: LinearComparison,
    },
    /// Pairwise distinct integer values; empty/single is true.
    AllDifferent { variables: Vec<IntVariableId> },
    /// Exact-arity allowed rows; duplicate rows are noncanonical.
    AllowedTable {
        variables: Vec<IntVariableId>,
        rows: Vec<Vec<i64>>,
    },
    /// Exact-arity forbidden rows; duplicate rows are noncanonical.
    ForbiddenTable {
        variables: Vec<IntVariableId>,
        rows: Vec<Vec<i64>>,
    },
    /// `target == values[index]`; out-of-range index makes the constraint false.
    Element {
        index: IntVariableId,
        values: Vec<i64>,
        target: IntVariableId,
    },
    /// `target == min(inputs)`; inputs are nonempty.
    Min {
        target: IntVariableId,
        inputs: Vec<IntVariableId>,
    },
    /// `target == max(inputs)`; inputs are nonempty.
    Max {
        target: IntVariableId,
        inputs: Vec<IntVariableId>,
    },
    /// Integer equality helper.
    Equality {
        left: IntVariableId,
        right: IntVariableId,
    },
    /// `target == abs(left - right)` with checked `i64` subtraction/absolute value.
    AbsDifference {
        target: IntVariableId,
        left: IntVariableId,
        right: IntVariableId,
    },
    /// Present half-open intervals do not overlap; absent intervals are ignored.
    NoOverlap { intervals: Vec<IntervalVariableId> },
    /// At every integer instant, the checked sum of nonnegative demands for present,
    /// overlapping half-open intervals is at most nonnegative `capacity`.
    Cumulative {
        intervals: Vec<IntervalVariableId>,
        demands: Vec<i64>,
        capacity: i64,
    },
}

impl Constraint {
    /// Canonical disjunction.
    #[must_use]
    pub fn bool_or(mut literals: Vec<Literal>) -> Self {
        canonical_literals(&mut literals);
        Self::BoolOr { literals }
    }

    /// Canonical conjunction.
    #[must_use]
    pub fn bool_and(mut literals: Vec<Literal>) -> Self {
        canonical_literals(&mut literals);
        Self::BoolAnd { literals }
    }

    /// Canonical at-most-one set.
    #[must_use]
    pub fn at_most_one(mut literals: Vec<Literal>) -> Self {
        canonical_literals(&mut literals);
        Self::AtMostOne { literals }
    }

    /// Canonical exactly-one set.
    #[must_use]
    pub fn exactly_one(mut literals: Vec<Literal>) -> Self {
        canonical_literals(&mut literals);
        Self::ExactlyOne { literals }
    }

    /// Canonical bounded cardinality.
    ///
    /// # Errors
    /// Rejects invalid bounds after literal deduplication.
    pub fn cardinality(mut literals: Vec<Literal>, min: u64, max: u64) -> Result<Self, ModelError> {
        canonical_literals(&mut literals);
        let len = u64::try_from(literals.len()).map_err(|_| ModelError::Shape)?;
        if min > max || max > len {
            return Err(ModelError::Shape);
        }
        Ok(Self::CardinalityRange { literals, min, max })
    }

    /// Canonical all-different set.
    #[must_use]
    pub fn all_different(mut variables: Vec<IntVariableId>) -> Self {
        variables.sort();
        variables.dedup();
        Self::AllDifferent { variables }
    }

    /// Canonical allowed table.
    ///
    /// # Errors
    /// Rejects any row whose arity differs from `variables.len()`.
    pub fn allowed_table(
        variables: Vec<IntVariableId>,
        mut rows: Vec<Vec<i64>>,
    ) -> Result<Self, ModelError> {
        canonical_table(&variables, &mut rows)?;
        Ok(Self::AllowedTable { variables, rows })
    }

    /// Canonical forbidden table.
    ///
    /// # Errors
    /// Rejects any row whose arity differs from `variables.len()`.
    pub fn forbidden_table(
        variables: Vec<IntVariableId>,
        mut rows: Vec<Vec<i64>>,
    ) -> Result<Self, ModelError> {
        canonical_table(&variables, &mut rows)?;
        Ok(Self::ForbiddenTable { variables, rows })
    }

    /// Canonical minimum helper.
    ///
    /// # Errors
    /// Rejects an empty input set.
    pub fn min(target: IntVariableId, mut inputs: Vec<IntVariableId>) -> Result<Self, ModelError> {
        if inputs.is_empty() {
            return Err(ModelError::Shape);
        }
        inputs.sort();
        inputs.dedup();
        Ok(Self::Min { target, inputs })
    }

    /// Canonical maximum helper.
    ///
    /// # Errors
    /// Rejects an empty input set.
    pub fn max(target: IntVariableId, mut inputs: Vec<IntVariableId>) -> Result<Self, ModelError> {
        if inputs.is_empty() {
            return Err(ModelError::Shape);
        }
        inputs.sort();
        inputs.dedup();
        Ok(Self::Max { target, inputs })
    }

    /// Canonical no-overlap set.
    #[must_use]
    pub fn no_overlap(mut intervals: Vec<IntervalVariableId>) -> Self {
        intervals.sort();
        intervals.dedup();
        Self::NoOverlap { intervals }
    }

    /// Canonical cumulative constraint.
    ///
    /// # Errors
    /// Rejects length mismatch, negative demand/capacity, duplicate interval IDs, or demand
    /// summation overflow.
    pub fn cumulative(
        intervals: Vec<IntervalVariableId>,
        demands: Vec<i64>,
        capacity: i64,
    ) -> Result<Self, ModelError> {
        if intervals.len() != demands.len()
            || capacity < 0
            || demands.iter().any(|demand| *demand < 0)
        {
            return Err(ModelError::Shape);
        }
        let mut pairs: Vec<_> = intervals.into_iter().zip(demands).collect();
        pairs.sort_by(|left, right| left.0.cmp(&right.0));
        if pairs.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(ModelError::Shape);
        }
        let mut sum = 0_i64;
        for (_, demand) in &pairs {
            sum = sum
                .checked_add(*demand)
                .ok_or(ModelError::ArithmeticOverflow)?;
        }
        let (intervals, demands) = pairs.into_iter().unzip();
        Ok(Self::Cumulative {
            intervals,
            demands,
            capacity,
        })
    }
}

fn canonical_literals(literals: &mut Vec<Literal>) {
    literals.sort();
    literals.dedup();
}

fn canonical_table(
    variables: &[IntVariableId],
    rows: &mut Vec<Vec<i64>>,
) -> Result<(), ModelError> {
    if rows.iter().any(|row| row.len() != variables.len()) {
        return Err(ModelError::Shape);
    }
    rows.sort();
    rows.dedup();
    Ok(())
}

/// Constraint plus common activation, explanation, and classification data.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConstraintRecord {
    /// Stable identity.
    pub id: PlanningConstraintId,
    /// Primitive semantics.
    pub body: Constraint,
    /// Conjunctive activation literals. Empty means always active.
    pub enforcement: Vec<Literal>,
    /// Required provenance.
    pub provenance: ProvenanceId,
    /// Canonical tags.
    pub tags: Vec<ConstraintTag>,
}

impl ConstraintRecord {
    /// Canonicalizes common commutative collections and collections inside the primitive.
    pub fn canonicalize(&mut self) {
        canonical_literals(&mut self.enforcement);
        self.tags.sort();
        self.tags.dedup();
        match &mut self.body {
            Constraint::BoolOr { literals }
            | Constraint::BoolAnd { literals }
            | Constraint::AtMostOne { literals }
            | Constraint::ExactlyOne { literals }
            | Constraint::CardinalityRange { literals, .. } => canonical_literals(literals),
            Constraint::AllDifferent { variables }
            | Constraint::Min {
                inputs: variables, ..
            }
            | Constraint::Max {
                inputs: variables, ..
            } => {
                variables.sort();
                variables.dedup();
            }
            Constraint::AllowedTable { rows, .. } | Constraint::ForbiddenTable { rows, .. } => {
                rows.sort();
                rows.dedup();
            }
            Constraint::NoOverlap { intervals } => {
                intervals.sort();
                intervals.dedup();
            }
            Constraint::Cumulative {
                intervals, demands, ..
            } => {
                let mut pairs: Vec<_> = intervals.drain(..).zip(demands.drain(..)).collect();
                pairs.sort_by(|left, right| left.0.cmp(&right.0));
                let (new_intervals, new_demands) = pairs.into_iter().unzip();
                *intervals = new_intervals;
                *demands = new_demands;
            }
            Constraint::Implication { .. }
            | Constraint::Equivalence { .. }
            | Constraint::LinearComparison(_)
            | Constraint::ReifiedLinearComparison { .. }
            | Constraint::Element { .. }
            | Constraint::Equality { .. }
            | Constraint::AbsDifference { .. } => {}
        }
    }
}

/// Explicit non-negative objective contribution interpretation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectiveTermKind {
    /// Cost to minimize within the level's direction contract.
    Penalty,
    /// Satisfaction reward.
    Reward,
}

/// Bounded objective expression contribution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectiveTerm {
    /// Stable identity.
    pub id: ObjectiveTermId,
    /// Linear integer expression.
    pub expression: LinearExpression,
    /// Penalty or reward interpretation.
    pub kind: ObjectiveTermKind,
    /// Stable explanatory category.
    pub category: ScoreCategoryId,
    /// Required provenance.
    pub provenance: ProvenanceId,
}

/// One exact, finite objective level. Vector position—not ID sort—defines precedence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectiveLevel {
    /// Stable identity.
    pub id: ObjectiveLevelId,
    /// Comparison direction.
    pub direction: OptimizationDirection,
    /// Proven exact finite lower bound.
    pub lower_bound: i64,
    /// Proven exact finite upper bound.
    pub upper_bound: i64,
    /// Canonically ID-sorted terms.
    pub terms: Vec<ObjectiveTerm>,
    /// Required provenance for the level itself.
    pub provenance: ProvenanceId,
}

/// Ordered lexicographic objective plan.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectivePlan {
    /// Highest precedence first; order is semantic and is preserved by canonicalization.
    pub levels: Vec<ObjectiveLevel>,
}

/// One candidate assumption literal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Assumption {
    /// Stable identity.
    pub id: AssumptionId,
    /// Boolean literal.
    pub literal: Literal,
    /// Required provenance.
    pub provenance: ProvenanceId,
}

/// Typed, bounded provenance parameter; arbitrary JSON is intentionally impossible.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "type",
    content = "value",
    deny_unknown_fields
)]
pub enum ProvenanceParameter {
    /// Boolean parameter.
    Boolean(bool),
    /// Integer parameter.
    Integer(i64),
    /// Bounded non-display identity or localization text parameter.
    Text(String),
    /// Stable entity reference.
    Entity(DomainEntityRef),
}

/// Provenance source classification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProvenanceSourceKind {
    /// Required domain rule.
    RequiredRule,
    /// Preference rule.
    Preference,
    /// Domain fact.
    Fact,
    /// Compiler-generated structural relation with a user-relevant parent.
    Derived,
    /// Projection relation.
    Projection,
}

/// One acyclic provenance record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProvenanceRecord {
    /// Stable identity.
    pub id: ProvenanceId,
    /// Source kind.
    pub source_kind: ProvenanceSourceKind,
    /// Stable domain source ID, never display text.
    pub source_id: String,
    /// Affected stable entities in canonical order.
    pub entity_refs: Vec<DomainEntityRef>,
    /// Stable localization key.
    pub message_key: String,
    /// Canonically key-sorted typed parameters.
    pub parameters: BTreeMap<String, ProvenanceParameter>,
    /// Optional parent. Parent chains must be acyclic and no deeper than the limit.
    pub parent: Option<ProvenanceId>,
}

/// A projection expression with exact output type.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "type",
    content = "value",
    deny_unknown_fields
)]
pub enum ProjectionExpression {
    /// Boolean variable.
    Boolean(BoolVariableId),
    /// Integer variable.
    Integer(IntVariableId),
    /// Canonical integer linear expression; all referenced variables form one projection relation.
    Linear(LinearExpression),
    /// Optional or mandatory interval.
    Interval(IntervalVariableId),
    /// Explicit compile-time value.
    Constant(AssignmentValue),
}

/// Stable mapping into one normalized domain assignment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SolutionProjection {
    /// Stable projection identity.
    pub id: ProjectionId,
    /// Stable normalized assignment identity.
    pub assignment_id: DomainAssignmentId,
    /// Stable domain entity.
    pub entity: DomainEntityRef,
    /// Missing candidate input is an error when true; otherwise it projects `Absent`.
    pub required: bool,
    /// Typed expression.
    pub expression: ProjectionExpression,
    /// Required provenance.
    pub provenance: ProvenanceId,
}

/// Compile identity and explicit deterministic context.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningMetadata {
    /// Owning pack.
    pub pack_id: PackId,
    /// Scenario identity.
    pub scenario_id: ScenarioId,
    /// Immutable revision.
    pub scenario_revision: u64,
    /// Domain projection contract version.
    pub projection_version: u32,
    /// Compiler identity.
    pub compiler_id: CompilerId,
    /// Compiler version string.
    pub compiler_version: String,
    /// Explicit semantic compile context included in canonical hashing.
    pub compile_metadata: BTreeMap<MetadataKey, ProvenanceParameter>,
    /// Nonsemantic display metadata excluded from canonical hashing.
    pub display_text: BTreeMap<String, String>,
}

/// Capability required by a mathematical feature.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    BoolOr,
    BoolAnd,
    Implication,
    Equivalence,
    AtMostOne,
    ExactlyOne,
    CardinalityRange,
    LinearComparison,
    ReifiedLinearComparison,
    AllDifferent,
    AllowedTable,
    ForbiddenTable,
    Element,
    Min,
    Max,
    Equality,
    AbsDifference,
    NoOverlap,
    Cumulative,
    OptionalIntervals,
    ObjectivePenalty,
    ObjectiveReward,
    Assumptions,
    BooleanProjection,
    IntegerProjection,
    IntervalProjection,
    AbsentProjection,
}

/// Explicit domain-side proof required before mathematical components may be split.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SplitAuthorization {
    /// Lowercase BLAKE3 hash of the exact computed component graph.
    pub component_hash: String,
    /// Stable explicit domain merge contract, not prose.
    pub domain_merge_contract: String,
    /// Domain/compiler assertion that projection evaluation and domain invariants do not cross
    /// computed components.
    pub projection_independent: bool,
}

/// Complete immutable schema-v1 planning problem.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningProblem {
    /// Must equal [`PLANNING_IR_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Root variables sorted by typed ID string.
    pub variables: Vec<Variable>,
    /// Constraints sorted by ID.
    pub constraints: Vec<ConstraintRecord>,
    /// Ordered objective levels.
    pub objectives: ObjectivePlan,
    /// Assumptions sorted by ID.
    pub assumptions: Vec<Assumption>,
    /// Projections sorted by ID.
    pub projections: Vec<SolutionProjection>,
    /// Provenance records sorted by ID.
    pub provenance: Vec<ProvenanceRecord>,
    /// Compile identity/context.
    pub metadata: PlanningMetadata,
    /// Explicit required capabilities; validation rejects any omitted extracted capability.
    pub declared_capabilities: BTreeSet<Capability>,
    /// Optional split proof. Absence means unsplittable, even for disconnected mathematics.
    pub split_authorization: Option<SplitAuthorization>,
}

/// Deterministic post-route solve identity input.
///
/// Unlike `canonical_ir_hash`, this binds route, backend/adapter versions, and canonical
/// solve-option data.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SolveFingerprintInput {
    /// Pre-route canonical IR hash.
    pub canonical_ir_hash: String,
    /// Routed backend ID.
    pub backend_id: String,
    /// Backend implementation version.
    pub backend_version: String,
    /// Adapter version.
    pub adapter_version: String,
    /// Canonical serialized solve options supplied by the application boundary.
    pub canonical_options: Vec<u8>,
}

/// Shape construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelError {
    EmptyDomain,
    ReversedRange,
    Shape,
    ArithmeticOverflow,
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "planning model construction error: {self:?}")
    }
}

impl std::error::Error for ModelError {}
