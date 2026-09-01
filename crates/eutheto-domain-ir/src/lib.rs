//! Solver-independent domain result contracts.
//!
//! A [`NormalizedSolution`] contains projected domain assignments only. It records the
//! immutable scenario revision and projection contract version, but deliberately contains
//! no backend feasibility or objective claim. [`AcceptedResult::new`] is a data-integrity
//! gate over a separately produced [`VerificationReport`]; it is not a verifier,
//! orchestrator, or source of production authority.

use eutheto_types::{PackId, ScenarioId, SolutionId};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

/// Current normalized-solution wire schema.
pub const NORMALIZED_SOLUTION_SCHEMA_VERSION: u32 = 1;
/// Current independent-verification report wire schema.
pub const VERIFICATION_REPORT_SCHEMA_VERSION: u32 = 1;
/// Current accepted-result wire schema.
pub const ACCEPTED_RESULT_SCHEMA_VERSION: u32 = 1;
/// Longest domain IR stable identifier, in UTF-8 bytes.
pub const MAX_DOMAIN_ID_BYTES: usize = 160;

/// Invalid stable domain identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DomainIdError;

impl fmt::Display for DomainIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("identifier must be at most 160 bytes and contain two or more lowercase ASCII namespace segments using letters, digits, '_' or '-'")
    }
}

impl std::error::Error for DomainIdError {}

fn valid_id(value: &str) -> bool {
    value.len() <= MAX_DOMAIN_ID_BYTES
        && value.split('.').count() >= 2
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_'
                        || byte == b'-'
                })
                && segment
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && segment
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
}

macro_rules! domain_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Validates and creates an identifier.
            ///
            /// # Errors
            /// Returns [`DomainIdError`] if `value` is not canonical.
            pub fn new(value: impl Into<String>) -> Result<Self, DomainIdError> {
                let value = value.into();
                if valid_id(&value) {
                    Ok(Self(value))
                } else {
                    Err(DomainIdError)
                }
            }

            /// Returns the canonical identifier.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = DomainIdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

domain_id!(
    DomainAssignmentId,
    "Stable identity of one projected domain assignment."
);
domain_id!(DomainEntityId, "Stable identity of a domain entity.");
domain_id!(
    DomainEntityKindId,
    "Stable identity of a domain entity kind."
);
domain_id!(
    DomainEvidenceId,
    "Stable identity of optional backend or verifier evidence."
);
domain_id!(
    ScoreLevelId,
    "Stable identity of a lexicographic score level."
);
domain_id!(ScoreCategoryId, "Stable identity of a score category.");
domain_id!(
    VerificationIssueId,
    "Stable identity of a verification issue."
);

/// Stable typed reference to an entity. Display text is never identity.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainEntityRef {
    /// Entity kind.
    pub kind: DomainEntityKindId,
    /// Entity identity within the kind.
    pub id: DomainEntityId,
}

/// A present half-open interval `[start, end)`.
///
/// Duration must be non-negative and `start + duration == end` in checked `i64`
/// arithmetic. A zero-duration interval occupies no time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignedInterval {
    /// Inclusive start instant.
    pub start: i64,
    /// Non-negative duration.
    pub duration: i64,
    /// Exclusive end instant.
    pub end: i64,
}

impl AssignedInterval {
    /// Creates a coherent interval.
    ///
    /// # Errors
    /// Returns [`DomainContractError::InvalidInterval`] for a negative duration,
    /// arithmetic overflow, or an incoherent end.
    pub fn new(start: i64, duration: i64, end: i64) -> Result<Self, DomainContractError> {
        if duration < 0 || start.checked_add(duration) != Some(end) {
            Err(DomainContractError::InvalidInterval)
        } else {
            Ok(Self {
                start,
                duration,
                end,
            })
        }
    }

    /// Validates the interval invariant.
    ///
    /// # Errors
    /// Returns [`DomainContractError::InvalidInterval`] when incoherent.
    pub fn validate(self) -> Result<(), DomainContractError> {
        Self::new(self.start, self.duration, self.end).map(|_| ())
    }
}

/// Typed projected value. `Absent` is distinct from `false`, zero, and a zero-duration interval.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "type",
    content = "value",
    deny_unknown_fields
)]
pub enum AssignmentValue {
    /// Boolean assignment.
    Boolean(bool),
    /// Signed integer assignment.
    Integer(i64),
    /// Present interval assignment.
    Interval(AssignedInterval),
    /// Explicit absence of an optional projected value.
    Absent,
}

/// One stable normalized assignment and optional non-authoritative evidence references.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainAssignment {
    /// Assignment identity.
    pub id: DomainAssignmentId,
    /// Entity represented by the assignment.
    pub entity: DomainEntityRef,
    /// Typed value.
    pub value: AssignmentValue,
    /// Stable evidence references. These do not establish correctness.
    pub evidence: Vec<DomainEvidenceId>,
}

/// Versioned, backend-independent projection result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NormalizedSolution {
    /// Must equal [`NORMALIZED_SOLUTION_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Domain pack that owns the projection contract.
    pub pack_id: PackId,
    /// Immutable scenario identity.
    pub scenario_id: ScenarioId,
    /// Immutable revision solved.
    pub scenario_revision: u64,
    /// Domain-defined projection contract version.
    pub projection_version: u32,
    /// Stable solution identity.
    pub solution_id: SolutionId,
    /// Assignments in ascending assignment-ID order.
    pub assignments: Vec<DomainAssignment>,
}

impl NormalizedSolution {
    /// Strictly parses and validates a normalized solution.
    ///
    /// # Errors
    /// Rejects malformed JSON, unknown fields, unknown versions, duplicate or noncanonical
    /// assignment/evidence ordering, and incoherent intervals.
    pub fn from_json(bytes: &[u8]) -> Result<Self, DomainContractError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| DomainContractError::MalformedJson(error.to_string()))?;
        value.validate()?;
        Ok(value)
    }

    /// Validates schema and canonical ordering.
    ///
    /// # Errors
    /// Returns a typed contract error for the first invalid invariant.
    pub fn validate(&self) -> Result<(), DomainContractError> {
        if self.schema_version != NORMALIZED_SOLUTION_SCHEMA_VERSION {
            return Err(DomainContractError::UnsupportedVersion(self.schema_version));
        }
        let mut prior: Option<&DomainAssignmentId> = None;
        for assignment in &self.assignments {
            if prior.is_some_and(|id| id >= &assignment.id) {
                return Err(DomainContractError::NonCanonicalAssignments);
            }
            prior = Some(&assignment.id);
            if let AssignmentValue::Interval(interval) = assignment.value {
                interval.validate()?;
            }
            if !strictly_sorted_unique(&assignment.evidence) {
                return Err(DomainContractError::NonCanonicalEvidence);
            }
        }
        Ok(())
    }

    /// Sorts assignments/evidence and rejects duplicate assignment IDs.
    ///
    /// # Errors
    /// Returns [`DomainContractError::DuplicateAssignment`] for duplicate IDs or an invalid
    /// interval error.
    pub fn canonicalize(&mut self) -> Result<(), DomainContractError> {
        self.assignments
            .sort_by(|left, right| left.id.cmp(&right.id));
        for pair in self.assignments.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(DomainContractError::DuplicateAssignment(pair[0].id.clone()));
            }
        }
        for assignment in &mut self.assignments {
            assignment.evidence.sort();
            assignment.evidence.dedup();
        }
        self.validate()
    }
}

fn strictly_sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

/// Direction of one authoritative score level.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OptimizationDirection {
    /// Smaller values are better.
    Minimize,
    /// Larger values are better.
    Maximize,
}

/// One ordered score level with a deterministic category breakdown.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScoreLevelValue {
    /// Stable level identity.
    pub level_id: ScoreLevelId,
    /// Authoritative value.
    pub value: i64,
    /// Comparison direction.
    pub direction: OptimizationDirection,
    /// Deterministic explanatory breakdown. It need not sum to `value` unless the pack says so.
    pub category_breakdown: BTreeMap<ScoreCategoryId, i64>,
}

/// Ordered lexicographic authoritative score.
///
/// Feasibility is compared first and minimized; accepted results require it to be zero.
/// Level vector position defines precedence and identities/directions must match before values
/// can be compared.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScoreVector {
    /// Zero means independently verified feasible; positive values are violations.
    pub feasibility: i64,
    /// Highest precedence first.
    pub levels: Vec<ScoreLevelValue>,
}

impl ScoreVector {
    /// Lexicographically compares two scores.
    ///
    /// [`Ordering::Less`] means `self` is better. Feasibility is minimized, then each level
    /// follows its declared direction.
    ///
    /// # Errors
    /// Rejects negative feasibility or level identity/direction mismatch.
    pub fn compare(&self, other: &Self) -> Result<Ordering, DomainContractError> {
        self.validate_shape()?;
        other.validate_shape()?;
        if self.levels.len() != other.levels.len() {
            return Err(DomainContractError::ScoreShapeMismatch);
        }
        let feasibility = self.feasibility.cmp(&other.feasibility);
        if feasibility != Ordering::Equal {
            return Ok(feasibility);
        }
        for (left, right) in self.levels.iter().zip(&other.levels) {
            if left.level_id != right.level_id || left.direction != right.direction {
                return Err(DomainContractError::ScoreShapeMismatch);
            }
            let ordering = match left.direction {
                OptimizationDirection::Minimize => left.value.cmp(&right.value),
                OptimizationDirection::Maximize => right.value.cmp(&left.value),
            };
            if ordering != Ordering::Equal {
                return Ok(ordering);
            }
        }
        Ok(Ordering::Equal)
    }

    /// Validates feasibility and unique ordered level identities.
    ///
    /// # Errors
    /// Rejects negative feasibility or duplicate level IDs.
    pub fn validate_shape(&self) -> Result<(), DomainContractError> {
        if self.feasibility < 0 {
            return Err(DomainContractError::NegativeFeasibility);
        }
        let mut ids = BTreeSet::new();
        if self.levels.iter().any(|level| !ids.insert(&level.level_id)) {
            return Err(DomainContractError::DuplicateScoreLevel);
        }
        Ok(())
    }
}

/// Severity of an independently verified domain finding.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VerificationSeverity {
    /// Candidate cannot be accepted.
    Error,
    /// Informational warning that does not make the candidate infeasible.
    Warning,
}

/// Stable verification finding; `message_key` is localization data, not identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationIssue {
    /// Stable issue identity.
    pub id: VerificationIssueId,
    /// Severity.
    pub severity: VerificationSeverity,
    /// Stable localization key.
    pub message_key: String,
    /// Related entities in canonical order.
    pub entities: Vec<DomainEntityRef>,
    /// Optional evidence records.
    pub evidence: Vec<DomainEvidenceId>,
}

/// Data emitted by an independent domain verifier.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationReport {
    /// Must equal [`VERIFICATION_REPORT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Scenario revision that was verified.
    pub scenario_revision: u64,
    /// True only if verification completed and found no required-rule violation.
    pub feasible: bool,
    /// Stable findings in ascending ID order.
    pub issues: Vec<VerificationIssue>,
    /// Authoritatively recomputed score, present only after complete verification.
    pub score: Option<ScoreVector>,
}

impl VerificationReport {
    /// Validates report shape and acceptance consistency.
    ///
    /// # Errors
    /// Rejects unknown versions, noncanonical findings, inconsistent feasibility, or score.
    pub fn validate(&self) -> Result<(), DomainContractError> {
        if self.schema_version != VERIFICATION_REPORT_SCHEMA_VERSION {
            return Err(DomainContractError::UnsupportedVersion(self.schema_version));
        }
        if !strictly_sorted_unique_by(&self.issues, |issue| &issue.id) {
            return Err(DomainContractError::NonCanonicalVerificationIssues);
        }
        let has_error = self
            .issues
            .iter()
            .any(|issue| issue.severity == VerificationSeverity::Error);
        if self.feasible == has_error {
            return Err(DomainContractError::InconsistentVerification);
        }
        if let Some(score) = &self.score {
            score.validate_shape()?;
            if self.feasible != (score.feasibility == 0) {
                return Err(DomainContractError::InconsistentVerification);
            }
        }
        Ok(())
    }
}

fn strictly_sorted_unique_by<T, K: Ord>(values: &[T], key: impl Fn(&T) -> &K) -> bool {
    values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

/// Validated pairing of a normalized solution and independent verification report.
///
/// Construction only checks a report already produced by trusted application policy. It does
/// not run verification, choose authority, accept backend claims, persist data, or authorize
/// display/share side effects.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcceptedResult {
    /// Must equal [`ACCEPTED_RESULT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Projected candidate.
    pub solution: NormalizedSolution,
    /// Independent report for exactly the same scenario revision.
    pub verification: VerificationReport,
}

impl AcceptedResult {
    /// Creates the accepted-result data contract after validating both inputs.
    ///
    /// # Errors
    /// Rejects invalid inputs, revision mismatch, infeasibility, missing score, or nonzero
    /// verified feasibility.
    pub fn new(
        solution: NormalizedSolution,
        verification: VerificationReport,
    ) -> Result<Self, DomainContractError> {
        solution.validate()?;
        verification.validate()?;
        if solution.scenario_revision != verification.scenario_revision {
            return Err(DomainContractError::RevisionMismatch);
        }
        if !verification.feasible {
            return Err(DomainContractError::NotVerifiedFeasible);
        }
        let score = verification
            .score
            .as_ref()
            .ok_or(DomainContractError::MissingVerifiedScore)?;
        if score.feasibility != 0 {
            return Err(DomainContractError::NotVerifiedFeasible);
        }
        Ok(Self {
            schema_version: ACCEPTED_RESULT_SCHEMA_VERSION,
            solution,
            verification,
        })
    }

    /// Validates a deserialized accepted result with the same rules as construction.
    ///
    /// # Errors
    /// Returns the first violated contract invariant.
    pub fn validate(&self) -> Result<(), DomainContractError> {
        if self.schema_version != ACCEPTED_RESULT_SCHEMA_VERSION {
            return Err(DomainContractError::UnsupportedVersion(self.schema_version));
        }
        Self::new(self.solution.clone(), self.verification.clone()).map(|_| ())
    }
}

/// Domain result contract validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DomainContractError {
    /// Serialized schema version is unsupported.
    UnsupportedVersion(u32),
    /// JSON cannot be parsed strictly.
    MalformedJson(String),
    /// Present interval is incoherent or overflows.
    InvalidInterval,
    /// Assignments are not strictly ID-sorted.
    NonCanonicalAssignments,
    /// Duplicate assignment identity.
    DuplicateAssignment(DomainAssignmentId),
    /// Evidence references are not strictly sorted and unique.
    NonCanonicalEvidence,
    /// Score feasibility is negative.
    NegativeFeasibility,
    /// Score level identity occurs more than once.
    DuplicateScoreLevel,
    /// Compared score vectors do not have identical ordered identities/directions.
    ScoreShapeMismatch,
    /// Verification issues are not strictly ID-sorted.
    NonCanonicalVerificationIssues,
    /// Feasible flag, findings, and score disagree.
    InconsistentVerification,
    /// Solution and verification refer to different revisions.
    RevisionMismatch,
    /// Verification did not establish feasibility.
    NotVerifiedFeasible,
    /// A feasible report omitted its authoritative score.
    MissingVerifiedScore,
}

impl fmt::Display for DomainContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "domain IR contract error: {self:?}")
    }
}

impl std::error::Error for DomainContractError {}
