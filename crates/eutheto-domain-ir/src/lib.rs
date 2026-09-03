//! Solver-independent domain result contracts.
//!
//! A [`NormalizedSolution`] contains projected domain assignments only. It records the
//! immutable scenario revision and projection contract version, but deliberately contains
//! no backend feasibility or objective claim. [`AcceptedResult::new`] is a data-integrity
//! gate over a separately produced [`VerificationReport`]; it is not a verifier,
//! orchestrator, or source of production authority.

use eutheto_types::{PackId, REVISION_MAX_V1, RuleId, ScenarioId, SolutionId};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

/// Current normalized-solution wire schema.
pub const NORMALIZED_SOLUTION_SCHEMA_VERSION: u32 = 1;
/// Current verification-scope wire schema.
pub const VERIFICATION_SCOPE_SCHEMA_VERSION: u32 = 1;
/// Current verification-context wire schema.
pub const VERIFICATION_CONTEXT_SCHEMA_VERSION: u32 = 1;
/// Current independent-verification report wire schema.
pub const VERIFICATION_REPORT_SCHEMA_VERSION: u32 = 2;
/// Current accepted-result wire schema.
pub const ACCEPTED_RESULT_SCHEMA_VERSION: u32 = 2;
/// Maximum required rules or evaluations in one verification contract.
pub const MAX_VERIFICATION_RULES: usize = 100_000;
/// Maximum warnings in one verification report.
pub const MAX_VERIFICATION_WARNINGS: usize = 10_000;
/// Maximum metrics in one verification report.
pub const MAX_VERIFICATION_METRICS: usize = 1_024;
/// Maximum facts in one typed fact map.
pub const MAX_VERIFICATION_FACTS: usize = 64;
/// Maximum affected entities in one verification record.
pub const MAX_VERIFICATION_ENTITIES: usize = 256;
/// Maximum evidence references in one rule evaluation.
pub const MAX_VERIFICATION_EVIDENCE: usize = 64;
/// Maximum UTF-8 bytes in one verification text value.
pub const MAX_VERIFICATION_TEXT_BYTES: usize = 4_096;
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
domain_id!(
    VerificationFactId,
    "Stable identity of a typed verification fact or message parameter."
);
domain_id!(
    MetricId,
    "Stable identity of a verifier-owned domain metric."
);
domain_id!(
    VerificationWarningId,
    "Stable identity of a non-acceptance-blocking verification warning."
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
        validate_revision(self.scenario_revision)?;
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

    /// Computes the lowercase BLAKE3 hash of the canonical V1 solution.
    ///
    /// # Errors
    /// Rejects an invalid solution or an unexpected serialization failure.
    pub fn canonical_hash(&self) -> Result<String, DomainContractError> {
        self.validate()?;
        canonical_hash(self)
    }
}

fn strictly_sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

/// Computes a lowercase BLAKE3 digest for caller-defined canonical bytes.
#[must_use]
pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn canonical_hash(value: &impl Serialize) -> Result<String, DomainContractError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| DomainContractError::CanonicalSerialization(error.to_string()))?;
    Ok(blake3_hex(&bytes))
}

fn checksum_without_field(
    value: &impl Serialize,
    field: &'static str,
) -> Result<String, DomainContractError> {
    let mut canonical = serde_json::to_value(value)
        .map_err(|error| DomainContractError::CanonicalSerialization(error.to_string()))?;
    canonical
        .as_object_mut()
        .and_then(|object| object.remove(field))
        .ok_or(DomainContractError::CanonicalSerialization(
            "checksum field is missing".to_owned(),
        ))?;
    canonical_hash(&canonical)
}

fn valid_blake3(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_revision(value: u64) -> Result<(), DomainContractError> {
    if value > REVISION_MAX_V1 {
        Err(DomainContractError::LimitExceeded("scenario revision"))
    } else {
        Ok(())
    }
}

fn validate_hash(value: &str) -> Result<(), DomainContractError> {
    if valid_blake3(value) {
        Ok(())
    } else {
        Err(DomainContractError::InvalidBlake3)
    }
}

fn validate_message_key(value: &str) -> Result<(), DomainContractError> {
    if valid_id(value) {
        Ok(())
    } else {
        Err(DomainContractError::InvalidMessageKey)
    }
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

/// Typed value in verifier facts and localizable message parameters.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "type",
    content = "value",
    deny_unknown_fields
)]
pub enum VerificationValue {
    /// Boolean fact.
    Boolean(bool),
    /// Signed integer fact.
    Integer(i64),
    /// Non-empty bounded UTF-8 text.
    Text(String),
    /// Stable affected entity.
    Entity(DomainEntityRef),
}

impl VerificationValue {
    fn validate(&self) -> Result<(), DomainContractError> {
        if let Self::Text(value) = self
            && (value.is_empty() || value.len() > MAX_VERIFICATION_TEXT_BYTES)
        {
            return Err(DomainContractError::InvalidVerificationText);
        }
        Ok(())
    }
}

/// Typed verifier-owned metric value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "type",
    content = "value",
    deny_unknown_fields
)]
pub enum MetricValue {
    /// Signed integer metric.
    Integer(i64),
    /// Exact rational metric; the denominator must be non-zero.
    Ratio {
        /// Signed numerator.
        numerator: i64,
        /// Positive denominator.
        denominator: u64,
    },
}

impl MetricValue {
    fn validate(&self) -> Result<(), DomainContractError> {
        if matches!(self, Self::Ratio { denominator: 0, .. }) {
            return Err(DomainContractError::InvalidMetricRatio);
        }
        Ok(())
    }
}

/// Semantic identity of one required rule in a verification scope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequiredRuleBinding {
    /// Stable scenario rule identity.
    pub rule_id: RuleId,
    /// Lowercase BLAKE3 hash of the rule/entity meaning evaluated by the verifier.
    pub semantic_hash: String,
}

/// Canonical V1 set of required rules for one immutable scenario revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationScope {
    /// Must equal [`VERIFICATION_SCOPE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Immutable scenario identity.
    pub scenario_id: ScenarioId,
    /// Immutable scenario revision.
    pub scenario_revision: u64,
    /// Required rules in ascending rule-ID order.
    pub required_rules: Vec<RequiredRuleBinding>,
    /// Lowercase BLAKE3 checksum of this scope excluding this field.
    pub checksum: String,
}

impl VerificationScope {
    /// Creates a canonical scope and computes its checksum.
    ///
    /// # Errors
    /// Rejects duplicate rules, malformed semantic hashes, or the configured rule bound.
    pub fn new(
        scenario_id: ScenarioId,
        scenario_revision: u64,
        mut required_rules: Vec<RequiredRuleBinding>,
    ) -> Result<Self, DomainContractError> {
        validate_revision(scenario_revision)?;
        if required_rules.len() > MAX_VERIFICATION_RULES {
            return Err(DomainContractError::LimitExceeded("required rules"));
        }
        required_rules.sort_by_key(|binding| binding.rule_id);
        for pair in required_rules.windows(2) {
            if pair[0].rule_id == pair[1].rule_id {
                return Err(DomainContractError::DuplicateRequiredRule(pair[0].rule_id));
            }
        }
        for binding in &required_rules {
            validate_hash(&binding.semantic_hash)?;
        }
        let mut scope = Self {
            schema_version: VERIFICATION_SCOPE_SCHEMA_VERSION,
            scenario_id,
            scenario_revision,
            required_rules,
            checksum: String::new(),
        };
        scope.checksum = checksum_without_field(&scope, "checksum")?;
        Ok(scope)
    }

    /// Strictly parses and validates a verification scope.
    ///
    /// # Errors
    /// Rejects malformed JSON and any scope invariant violation.
    pub fn from_json(bytes: &[u8]) -> Result<Self, DomainContractError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| DomainContractError::MalformedJson(error.to_string()))?;
        value.validate()?;
        Ok(value)
    }

    /// Validates version, bounds, canonical rule bindings, and checksum.
    ///
    /// # Errors
    /// Returns the first violated contract invariant.
    pub fn validate(&self) -> Result<(), DomainContractError> {
        if self.schema_version != VERIFICATION_SCOPE_SCHEMA_VERSION {
            return Err(DomainContractError::UnsupportedVersion(self.schema_version));
        }
        validate_revision(self.scenario_revision)?;
        if self.required_rules.len() > MAX_VERIFICATION_RULES {
            return Err(DomainContractError::LimitExceeded("required rules"));
        }
        if !strictly_sorted_unique_by(&self.required_rules, |binding| &binding.rule_id) {
            return Err(DomainContractError::NonCanonicalRequiredRules);
        }
        for binding in &self.required_rules {
            validate_hash(&binding.semantic_hash)?;
        }
        validate_hash(&self.checksum)?;
        if checksum_without_field(self, "checksum")? != self.checksum {
            return Err(DomainContractError::ChecksumMismatch);
        }
        Ok(())
    }
}

/// V1 immutable inputs and canonical bindings supplied to a domain verifier.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationContextV1 {
    /// Must equal [`VERIFICATION_CONTEXT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Immutable scenario identity.
    pub scenario_id: ScenarioId,
    /// Exact scenario revision being evaluated.
    pub evaluated_revision: u64,
    /// Lowercase BLAKE3 hash of the normalized scenario document.
    pub document_hash: String,
    /// Lowercase BLAKE3 hash of the canonical planning model.
    pub planning_model_hash: String,
    /// Lowercase BLAKE3 hash of the canonical normalized solution.
    pub normalized_solution_hash: String,
    /// Checksum of the exact required-rule scope.
    pub verification_scope_checksum: String,
}

impl VerificationContextV1 {
    /// Creates and validates a verifier context.
    ///
    /// # Errors
    /// Rejects any malformed lowercase BLAKE3 binding.
    pub fn new(
        scenario_id: ScenarioId,
        evaluated_revision: u64,
        document_hash: String,
        planning_model_hash: String,
        normalized_solution_hash: String,
        verification_scope_checksum: String,
    ) -> Result<Self, DomainContractError> {
        let value = Self {
            schema_version: VERIFICATION_CONTEXT_SCHEMA_VERSION,
            scenario_id,
            evaluated_revision,
            document_hash,
            planning_model_hash,
            normalized_solution_hash,
            verification_scope_checksum,
        };
        value.validate()?;
        Ok(value)
    }

    /// Strictly parses and validates a verification context.
    ///
    /// # Errors
    /// Rejects malformed JSON and any context invariant violation.
    pub fn from_json(bytes: &[u8]) -> Result<Self, DomainContractError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| DomainContractError::MalformedJson(error.to_string()))?;
        value.validate()?;
        Ok(value)
    }

    /// Validates the context version and all hash bindings.
    ///
    /// # Errors
    /// Returns the first violated contract invariant.
    pub fn validate(&self) -> Result<(), DomainContractError> {
        if self.schema_version != VERIFICATION_CONTEXT_SCHEMA_VERSION {
            return Err(DomainContractError::UnsupportedVersion(self.schema_version));
        }
        validate_revision(self.evaluated_revision)?;
        for binding in [
            &self.document_hash,
            &self.planning_model_hash,
            &self.normalized_solution_hash,
            &self.verification_scope_checksum,
        ] {
            validate_hash(binding)?;
        }
        Ok(())
    }
}

/// Independent result for one required domain rule.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleEvaluation {
    /// Stable required-rule identity.
    pub rule_id: RuleId,
    /// Whether the required rule is satisfied.
    pub satisfied: bool,
    /// Related entities in ascending canonical order.
    pub affected_entities: Vec<DomainEntityRef>,
    /// Stable localization key.
    pub message_key: String,
    /// Typed expected facts keyed by stable fact identity.
    pub expected: BTreeMap<VerificationFactId, VerificationValue>,
    /// Typed observed facts keyed by stable fact identity.
    pub observed: BTreeMap<VerificationFactId, VerificationValue>,
    /// Evidence references in ascending canonical order.
    pub evidence: Vec<DomainEvidenceId>,
}

impl RuleEvaluation {
    fn canonicalize(&mut self) -> Result<(), DomainContractError> {
        self.affected_entities.sort();
        if !strictly_sorted_unique(&self.affected_entities) && self.affected_entities.len() > 1 {
            return Err(DomainContractError::DuplicateAffectedEntity);
        }
        self.evidence.sort();
        if !strictly_sorted_unique(&self.evidence) && self.evidence.len() > 1 {
            return Err(DomainContractError::DuplicateVerificationEvidence);
        }
        self.validate()
    }

    /// Validates bounds, canonical collections, message identity, and typed facts.
    ///
    /// # Errors
    /// Returns the first violated record invariant.
    pub fn validate(&self) -> Result<(), DomainContractError> {
        validate_message_key(&self.message_key)?;
        if self.affected_entities.len() > MAX_VERIFICATION_ENTITIES {
            return Err(DomainContractError::LimitExceeded("affected entities"));
        }
        if !strictly_sorted_unique(&self.affected_entities) {
            return Err(DomainContractError::NonCanonicalAffectedEntities);
        }
        if self.evidence.len() > MAX_VERIFICATION_EVIDENCE {
            return Err(DomainContractError::LimitExceeded("verification evidence"));
        }
        if !strictly_sorted_unique(&self.evidence) {
            return Err(DomainContractError::NonCanonicalVerificationEvidence);
        }
        validate_fact_map(&self.expected)?;
        validate_fact_map(&self.observed)
    }
}

/// Non-blocking, typed verifier warning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationWarning {
    /// Stable warning identity.
    pub id: VerificationWarningId,
    /// Stable localization key.
    pub message_key: String,
    /// Related entities in ascending canonical order.
    pub affected_entities: Vec<DomainEntityRef>,
    /// Typed warning facts keyed by stable fact identity.
    pub facts: BTreeMap<VerificationFactId, VerificationValue>,
}

impl VerificationWarning {
    fn canonicalize(&mut self) -> Result<(), DomainContractError> {
        self.affected_entities.sort();
        if !strictly_sorted_unique(&self.affected_entities) && self.affected_entities.len() > 1 {
            return Err(DomainContractError::DuplicateAffectedEntity);
        }
        self.validate()
    }

    /// Validates bounds, canonical entities, message identity, and typed facts.
    ///
    /// # Errors
    /// Returns the first violated record invariant.
    pub fn validate(&self) -> Result<(), DomainContractError> {
        validate_message_key(&self.message_key)?;
        if self.affected_entities.len() > MAX_VERIFICATION_ENTITIES {
            return Err(DomainContractError::LimitExceeded("affected entities"));
        }
        if !strictly_sorted_unique(&self.affected_entities) {
            return Err(DomainContractError::NonCanonicalAffectedEntities);
        }
        validate_fact_map(&self.facts)
    }
}

fn validate_fact_map(
    facts: &BTreeMap<VerificationFactId, VerificationValue>,
) -> Result<(), DomainContractError> {
    if facts.len() > MAX_VERIFICATION_FACTS {
        return Err(DomainContractError::LimitExceeded("verification facts"));
    }
    for value in facts.values() {
        value.validate()?;
    }
    Ok(())
}

fn validate_metrics(metrics: &BTreeMap<MetricId, MetricValue>) -> Result<(), DomainContractError> {
    if metrics.len() > MAX_VERIFICATION_METRICS {
        return Err(DomainContractError::LimitExceeded("verification metrics"));
    }
    for value in metrics.values() {
        value.validate()?;
    }
    Ok(())
}

/// Current V2 independent-verification report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationReport {
    /// Must equal [`VERIFICATION_REPORT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// True exactly when every required-rule evaluation is satisfied.
    pub accepted: bool,
    /// Immutable scenario identity.
    pub scenario_id: ScenarioId,
    /// Exact scenario revision evaluated.
    pub evaluated_revision: u64,
    /// Scenario document hash copied from the verifier context.
    pub document_hash: String,
    /// Planning model hash copied from the verifier context.
    pub planning_model_hash: String,
    /// Normalized solution hash copied from the verifier context.
    pub normalized_solution_hash: String,
    /// Required-rule scope checksum copied from the verifier context.
    pub verification_scope_checksum: String,
    /// Every required-rule evaluation in ascending rule-ID order.
    pub required_rule_results: Vec<RuleEvaluation>,
    /// Authoritatively recomputed score.
    pub score: ScoreVector,
    /// Non-blocking warnings in ascending warning-ID order.
    pub warnings: Vec<VerificationWarning>,
    /// Typed verifier metrics.
    pub metrics: BTreeMap<MetricId, MetricValue>,
    /// Lowercase BLAKE3 checksum of this report excluding this field.
    pub checksum: String,
}

impl VerificationReport {
    /// Creates a canonical V2 report and computes acceptance and checksum.
    ///
    /// # Errors
    /// Rejects invalid bindings, duplicate records, bounds, facts, metrics, or score shape.
    pub fn new(
        context: &VerificationContextV1,
        mut required_rule_results: Vec<RuleEvaluation>,
        score: ScoreVector,
        mut warnings: Vec<VerificationWarning>,
        metrics: BTreeMap<MetricId, MetricValue>,
    ) -> Result<Self, DomainContractError> {
        context.validate()?;
        if required_rule_results.len() > MAX_VERIFICATION_RULES {
            return Err(DomainContractError::LimitExceeded("rule evaluations"));
        }
        for evaluation in &mut required_rule_results {
            evaluation.canonicalize()?;
        }
        required_rule_results.sort_by_key(|evaluation| evaluation.rule_id);
        for pair in required_rule_results.windows(2) {
            if pair[0].rule_id == pair[1].rule_id {
                return Err(DomainContractError::DuplicateRuleEvaluation(
                    pair[0].rule_id,
                ));
            }
        }
        if warnings.len() > MAX_VERIFICATION_WARNINGS {
            return Err(DomainContractError::LimitExceeded("verification warnings"));
        }
        for warning in &mut warnings {
            warning.canonicalize()?;
        }
        warnings.sort_by(|left, right| left.id.cmp(&right.id));
        for pair in warnings.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(DomainContractError::DuplicateVerificationWarning);
            }
        }
        score.validate_shape()?;
        validate_metrics(&metrics)?;
        let accepted = required_rule_results.iter().all(|result| result.satisfied);
        let mut report = Self {
            schema_version: VERIFICATION_REPORT_SCHEMA_VERSION,
            accepted,
            scenario_id: context.scenario_id,
            evaluated_revision: context.evaluated_revision,
            document_hash: context.document_hash.clone(),
            planning_model_hash: context.planning_model_hash.clone(),
            normalized_solution_hash: context.normalized_solution_hash.clone(),
            verification_scope_checksum: context.verification_scope_checksum.clone(),
            required_rule_results,
            score,
            warnings,
            metrics,
            checksum: String::new(),
        };
        report.checksum = checksum_without_field(&report, "checksum")?;
        Ok(report)
    }

    /// Strictly parses and validates a V2 report.
    ///
    /// # Errors
    /// Rejects malformed JSON and any report invariant violation.
    pub fn from_json(bytes: &[u8]) -> Result<Self, DomainContractError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| DomainContractError::MalformedJson(error.to_string()))?;
        value.validate()?;
        Ok(value)
    }

    /// Validates version, bindings, canonical content, acceptance, and checksum.
    ///
    /// # Errors
    /// Returns the first violated contract invariant.
    pub fn validate(&self) -> Result<(), DomainContractError> {
        if self.schema_version != VERIFICATION_REPORT_SCHEMA_VERSION {
            return Err(DomainContractError::UnsupportedVersion(self.schema_version));
        }
        validate_revision(self.evaluated_revision)?;
        for binding in [
            &self.document_hash,
            &self.planning_model_hash,
            &self.normalized_solution_hash,
            &self.verification_scope_checksum,
            &self.checksum,
        ] {
            validate_hash(binding)?;
        }
        if self.required_rule_results.len() > MAX_VERIFICATION_RULES {
            return Err(DomainContractError::LimitExceeded("rule evaluations"));
        }
        if !strictly_sorted_unique_by(&self.required_rule_results, |result| &result.rule_id) {
            return Err(DomainContractError::NonCanonicalRuleEvaluations);
        }
        for evaluation in &self.required_rule_results {
            evaluation.validate()?;
        }
        self.score.validate_shape()?;
        if self.warnings.len() > MAX_VERIFICATION_WARNINGS {
            return Err(DomainContractError::LimitExceeded("verification warnings"));
        }
        if !strictly_sorted_unique_by(&self.warnings, |warning| &warning.id) {
            return Err(DomainContractError::NonCanonicalVerificationWarnings);
        }
        for warning in &self.warnings {
            warning.validate()?;
        }
        validate_metrics(&self.metrics)?;
        let accepted = self
            .required_rule_results
            .iter()
            .all(|result| result.satisfied);
        if self.accepted != accepted {
            return Err(DomainContractError::InconsistentVerification);
        }
        if checksum_without_field(self, "checksum")? != self.checksum {
            return Err(DomainContractError::ChecksumMismatch);
        }
        Ok(())
    }
}

fn strictly_sorted_unique_by<T, K: Ord>(values: &[T], key: impl Fn(&T) -> &K) -> bool {
    values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

/// Current V2 validated pairing of a normalized solution and independent report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcceptedResult {
    /// Must equal [`ACCEPTED_RESULT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Projected candidate.
    pub solution: NormalizedSolution,
    /// Independent V2 report bound to this exact solution.
    pub verification: VerificationReport,
    /// Lowercase BLAKE3 checksum of this result excluding this field.
    pub checksum: String,
}

impl AcceptedResult {
    /// Validates and binds an accepted result, then computes its checksum.
    ///
    /// # Errors
    /// Rejects invalid inputs, a scenario/revision/hash mismatch, or a non-accepted report.
    pub fn new(
        solution: NormalizedSolution,
        verification: VerificationReport,
    ) -> Result<Self, DomainContractError> {
        validate_result_binding(&solution, &verification)?;
        let mut result = Self {
            schema_version: ACCEPTED_RESULT_SCHEMA_VERSION,
            solution,
            verification,
            checksum: String::new(),
        };
        result.checksum = checksum_without_field(&result, "checksum")?;
        Ok(result)
    }

    /// Strictly parses and validates a V2 accepted result.
    ///
    /// # Errors
    /// Rejects malformed JSON and any accepted-result invariant violation.
    pub fn from_json(bytes: &[u8]) -> Result<Self, DomainContractError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| DomainContractError::MalformedJson(error.to_string()))?;
        value.validate()?;
        Ok(value)
    }

    /// Validates version, solution/report binding, and result checksum.
    ///
    /// # Errors
    /// Returns the first violated contract invariant.
    pub fn validate(&self) -> Result<(), DomainContractError> {
        if self.schema_version != ACCEPTED_RESULT_SCHEMA_VERSION {
            return Err(DomainContractError::UnsupportedVersion(self.schema_version));
        }
        validate_result_binding(&self.solution, &self.verification)?;
        validate_hash(&self.checksum)?;
        if checksum_without_field(self, "checksum")? != self.checksum {
            return Err(DomainContractError::ChecksumMismatch);
        }
        Ok(())
    }
}

fn validate_result_binding(
    solution: &NormalizedSolution,
    verification: &VerificationReport,
) -> Result<(), DomainContractError> {
    solution.validate()?;
    verification.validate()?;
    if solution.scenario_id != verification.scenario_id
        || solution.scenario_revision != verification.evaluated_revision
        || solution.canonical_hash()? != verification.normalized_solution_hash
    {
        return Err(DomainContractError::VerificationBindingMismatch);
    }
    if !verification.accepted || verification.score.feasibility != 0 {
        return Err(DomainContractError::NotVerifiedFeasible);
    }
    Ok(())
}

/// Inert V1 verification severity retained only for legacy decoding.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LegacyVerificationSeverityV1 {
    /// Legacy acceptance-blocking issue.
    Error,
    /// Legacy non-blocking warning.
    Warning,
}

/// Inert exact V1 issue shape retained only for legacy decoding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyVerificationIssueV1 {
    /// Legacy issue identity.
    pub id: VerificationIssueId,
    /// Legacy severity.
    pub severity: LegacyVerificationSeverityV1,
    /// Legacy message key.
    pub message_key: String,
    /// Legacy related entities.
    pub entities: Vec<DomainEntityRef>,
    /// Legacy evidence references.
    pub evidence: Vec<DomainEvidenceId>,
}

/// Inert exact V1 report shape retained only for legacy decoding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyVerificationReportV1 {
    /// Legacy schema version.
    pub schema_version: u32,
    /// Legacy scenario revision.
    pub scenario_revision: u64,
    /// Legacy feasibility flag.
    pub feasible: bool,
    /// Legacy issues.
    pub issues: Vec<LegacyVerificationIssueV1>,
    /// Legacy optional score.
    pub score: Option<ScoreVector>,
}

/// Inert exact V1 accepted-result shape retained only for legacy decoding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyAcceptedResultV1 {
    /// Legacy schema version.
    pub schema_version: u32,
    /// Legacy projected candidate.
    pub solution: NormalizedSolution,
    /// Legacy verification report.
    pub verification: LegacyVerificationReportV1,
}

impl LegacyVerificationReportV1 {
    /// Strictly parses and validates the inert V1 report shape.
    ///
    /// # Errors
    /// Rejects malformed JSON, any version other than V1, noncanonical issues, or inconsistent
    /// legacy feasibility/score data.
    pub fn from_json(bytes: &[u8]) -> Result<Self, DomainContractError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| DomainContractError::MalformedJson(error.to_string()))?;
        value.validate()?;
        Ok(value)
    }

    /// Validates the released V1 report contract without granting current acceptance authority.
    ///
    /// # Errors
    /// Returns the first violated legacy invariant.
    pub fn validate(&self) -> Result<(), DomainContractError> {
        if self.schema_version != 1 {
            return Err(DomainContractError::UnsupportedVersion(self.schema_version));
        }
        if !strictly_sorted_unique_by(&self.issues, |issue| &issue.id) {
            return Err(DomainContractError::NonCanonicalRuleEvaluations);
        }
        let has_error = self
            .issues
            .iter()
            .any(|issue| issue.severity == LegacyVerificationSeverityV1::Error);
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

impl LegacyAcceptedResultV1 {
    /// Strictly parses and validates the inert V1 accepted-result shape.
    ///
    /// # Errors
    /// Rejects malformed JSON, any version other than V1, or invalid legacy bindings.
    pub fn from_json(bytes: &[u8]) -> Result<Self, DomainContractError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| DomainContractError::MalformedJson(error.to_string()))?;
        value.validate()?;
        Ok(value)
    }

    /// Validates released V1 data without converting it to a current accepted result.
    ///
    /// # Errors
    /// Returns the first violated legacy invariant.
    pub fn validate(&self) -> Result<(), DomainContractError> {
        if self.schema_version != 1 {
            return Err(DomainContractError::UnsupportedVersion(self.schema_version));
        }
        self.solution.validate()?;
        self.verification.validate()?;
        if self.solution.scenario_revision != self.verification.scenario_revision
            || !self.verification.feasible
            || self
                .verification
                .score
                .as_ref()
                .is_none_or(|score| score.feasibility != 0)
        {
            return Err(DomainContractError::NotVerifiedFeasible);
        }
        Ok(())
    }
}

/// Domain result contract validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DomainContractError {
    /// Serialized schema version is unsupported.
    UnsupportedVersion(u32),
    /// JSON cannot be parsed strictly.
    MalformedJson(String),
    /// Canonical JSON serialization unexpectedly failed.
    CanonicalSerialization(String),
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
    /// A BLAKE3 hash is not exactly 64 lowercase hexadecimal characters.
    InvalidBlake3,
    /// A deterministic checksum does not match the canonical payload.
    ChecksumMismatch,
    /// A configured verification collection bound was exceeded.
    LimitExceeded(&'static str),
    /// A stable localization message key is invalid.
    InvalidMessageKey,
    /// A verification text value is empty or exceeds its byte bound.
    InvalidVerificationText,
    /// A metric ratio has a zero denominator.
    InvalidMetricRatio,
    /// Required rule bindings are not strictly sorted and unique.
    NonCanonicalRequiredRules,
    /// Required rule identity occurs more than once.
    DuplicateRequiredRule(RuleId),
    /// Required-rule evaluations are not strictly sorted and unique.
    NonCanonicalRuleEvaluations,
    /// Required-rule identity occurs more than once in a report.
    DuplicateRuleEvaluation(RuleId),
    /// Affected entities are not strictly sorted and unique.
    NonCanonicalAffectedEntities,
    /// An affected entity occurs more than once.
    DuplicateAffectedEntity,
    /// Verification evidence is not strictly sorted and unique.
    NonCanonicalVerificationEvidence,
    /// A verification evidence reference occurs more than once.
    DuplicateVerificationEvidence,
    /// Warnings are not strictly sorted and unique.
    NonCanonicalVerificationWarnings,
    /// Warning identity occurs more than once.
    DuplicateVerificationWarning,
    /// Acceptance does not match complete required-rule satisfaction.
    InconsistentVerification,
    /// The report does not bind to the exact scenario, revision, and normalized solution.
    VerificationBindingMismatch,
    /// Verification did not establish acceptance with zero feasibility.
    NotVerifiedFeasible,
}

impl fmt::Display for DomainContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "domain IR contract error: {self:?}")
    }
}

impl std::error::Error for DomainContractError {}
