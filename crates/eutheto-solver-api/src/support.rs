use crate::{
    DEFERRED_BACKEND_CANDIDATES, PRODUCTION_BACKENDS, SUPPORT_FEATURES,
    SUPPORT_MATRIX_IR_SCHEMA_VERSION, SUPPORT_MATRIX_SCHEMA_VERSION, SolverCapabilities,
    SolverDescriptor,
};
use eutheto_types::BackendId;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

const MAX_FEATURE_ID_BYTES: usize = 160;
const MAX_REASON_BYTES: usize = 512;

/// Stable generated-matrix feature identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SupportFeatureId(String);

impl SupportFeatureId {
    /// Creates a validated support-feature identifier.
    ///
    /// # Errors
    ///
    /// Returns [`SupportMatrixError::InvalidFeatureId`] if the value is not a bounded, lowercase,
    /// dotted identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, SupportMatrixError> {
        let value = value.into();
        if valid_feature_id(&value) {
            Ok(Self(value))
        } else {
            Err(SupportMatrixError::InvalidFeatureId(value))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SupportFeatureId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for SupportFeatureId {
    type Err = SupportMatrixError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for SupportFeatureId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

fn valid_feature_id(value: &str) -> bool {
    value.len() >= 3
        && value.len() <= MAX_FEATURE_ID_BYTES
        && value.split('.').count() >= 2
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

/// Generated feature category. Unknown future categories cannot silently change meaning.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SupportFeatureCategory {
    Primitive,
    Objective,
    Projection,
    Solve,
}

/// Phase gate recorded by the matrix source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "gateId")]
pub enum SupportFeatureGate {
    Unconditional,
    Enabled(String),
}

/// One row in the generated support matrix.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupportFeature {
    pub id: SupportFeatureId,
    pub category: SupportFeatureCategory,
    pub gate: SupportFeatureGate,
}

/// Exact state of one backend/feature cell.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "support",
    deny_unknown_fields
)]
pub enum SupportCell {
    Supported {
        fixture_id: String,
    },
    Degraded {
        restriction_id: String,
        reason: String,
        remediation: String,
        fixture_id: String,
    },
    Unsupported {
        reason: String,
        remediation: String,
        fixture_id: String,
    },
}

impl SupportCell {
    #[must_use]
    pub const fn level(&self) -> CompatibilityLevel {
        match self {
            Self::Supported { .. } => CompatibilityLevel::Supported,
            Self::Degraded { .. } => CompatibilityLevel::Degraded,
            Self::Unsupported { .. } => CompatibilityLevel::Unsupported,
        }
    }

    fn validate(&self) -> bool {
        match self {
            Self::Supported { fixture_id } => valid_contract_text(fixture_id),
            Self::Degraded {
                restriction_id,
                reason,
                remediation,
                fixture_id,
            } => {
                valid_contract_text(restriction_id)
                    && valid_reason(reason)
                    && valid_reason(remediation)
                    && valid_contract_text(fixture_id)
            }
            Self::Unsupported {
                reason,
                remediation,
                fixture_id,
            } => {
                valid_reason(reason) && valid_reason(remediation) && valid_contract_text(fixture_id)
            }
        }
    }
}

fn valid_contract_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_FEATURE_ID_BYTES && !value.chars().any(char::is_control)
}

fn valid_reason(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_REASON_BYTES && !value.chars().any(char::is_control)
}

/// Overall preflight state. Degraded is compatible but must retain its warnings.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CompatibilityLevel {
    Supported,
    Degraded,
    Unsupported,
}

/// One exact model/option requirement presented to matrix preflight.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequiredFeature {
    pub id: SupportFeatureId,
    pub usage_count: u64,
    pub path: String,
}

/// Exact incompatible feature, reason, and remediation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnsupportedFeature {
    pub feature_id: SupportFeatureId,
    pub usage_count: u64,
    pub path: String,
    pub reason: String,
    pub remediation: String,
}

/// Exact supported-with-restriction feature retained for caller display.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompatibilityWarning {
    pub feature_id: SupportFeatureId,
    pub usage_count: u64,
    pub path: String,
    pub restriction_id: String,
    pub reason: String,
    pub remediation: String,
}

/// Redacted translation cost hint; never a time or quality promise.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelCostEstimate {
    pub variables: u64,
    pub constraints: u64,
    pub references: u64,
}

/// Deterministic compatibility result. Unsupported and degraded are never conflated.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompatibilityReport {
    pub level: CompatibilityLevel,
    pub unsupported_features: Vec<UnsupportedFeature>,
    pub warnings: Vec<CompatibilityWarning>,
    pub estimated_translation_cost: Option<ModelCostEstimate>,
}

impl CompatibilityReport {
    #[must_use]
    pub const fn compatible(&self) -> bool {
        !matches!(self.level, CompatibilityLevel::Unsupported)
    }
}

/// One complete backend column. Every matrix feature must have exactly one cell.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendSupportColumn {
    pub backend_id: BackendId,
    pub backend_version: String,
    pub adapter_version: String,
    pub cells: Vec<(SupportFeatureId, SupportCell)>,
}

/// Deferred backend candidate retained as explicitly unclaimed production metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeferredBackendCandidate {
    pub backend_id: BackendId,
    pub candidate_version: String,
    pub owning_phase: u32,
}

#[derive(Clone, Debug)]
struct ValidatedBackendColumn {
    backend_version: String,
    adapter_version: String,
    cells: BTreeMap<SupportFeatureId, SupportCell>,
}

/// Executable support matrix used by registration and preflight.
#[derive(Clone, Debug)]
pub struct CapabilityMatrix {
    schema_version: u32,
    planning_ir_schema_version: u32,
    features: BTreeMap<SupportFeatureId, SupportFeature>,
    backends: BTreeMap<BackendId, ValidatedBackendColumn>,
    deferred_candidates: Vec<DeferredBackendCandidate>,
}

impl CapabilityMatrix {
    /// Builds and validates an executable capability matrix.
    ///
    /// # Errors
    ///
    /// Returns an error if schema versions do not match, features or backend entries are invalid
    /// or duplicated, a backend column is incomplete, or deferred and production backend claims
    /// conflict.
    pub fn new(
        schema_version: u32,
        planning_ir_schema_version: u32,
        features: Vec<SupportFeature>,
        columns: Vec<BackendSupportColumn>,
        deferred_candidates: Vec<DeferredBackendCandidate>,
    ) -> Result<Self, SupportMatrixError> {
        if schema_version != SUPPORT_MATRIX_SCHEMA_VERSION {
            return Err(SupportMatrixError::UnsupportedMatrixVersion(schema_version));
        }
        if planning_ir_schema_version != SUPPORT_MATRIX_IR_SCHEMA_VERSION {
            return Err(SupportMatrixError::PlanningIrVersionMismatch(
                planning_ir_schema_version,
            ));
        }
        let mut feature_map = BTreeMap::new();
        for feature in features {
            let id = feature.id.clone();
            if feature_map.insert(id.clone(), feature).is_some() {
                return Err(SupportMatrixError::DuplicateFeature(id));
            }
        }
        let feature_ids: BTreeSet<_> = feature_map.keys().cloned().collect();
        for column in &columns {
            if !valid_contract_text(&column.backend_version) {
                return Err(SupportMatrixError::InvalidBackendVersion(
                    column.backend_id.clone(),
                ));
            }
            if !valid_contract_text(&column.adapter_version) {
                return Err(SupportMatrixError::InvalidAdapterVersion(
                    column.backend_id.clone(),
                ));
            }
        }
        let mut backends = BTreeMap::new();
        for column in columns {
            let mut cells = BTreeMap::new();
            for (feature, cell) in column.cells {
                if !cell.validate() {
                    return Err(SupportMatrixError::InvalidCell(feature));
                }
                if cells.insert(feature.clone(), cell).is_some() {
                    return Err(SupportMatrixError::DuplicateCell {
                        backend: column.backend_id.clone(),
                        feature,
                    });
                }
            }
            let actual: BTreeSet<_> = cells.keys().cloned().collect();
            if actual != feature_ids {
                return Err(SupportMatrixError::IncompleteColumn(column.backend_id));
            }
            let backend_id = column.backend_id.clone();
            if backends
                .insert(
                    backend_id.clone(),
                    ValidatedBackendColumn {
                        backend_version: column.backend_version,
                        adapter_version: column.adapter_version,
                        cells,
                    },
                )
                .is_some()
            {
                return Err(SupportMatrixError::DuplicateBackend(backend_id));
            }
        }
        let mut deferred_ids = BTreeSet::new();
        for candidate in &deferred_candidates {
            if backends.contains_key(&candidate.backend_id) {
                return Err(SupportMatrixError::DeferredBackendClaimed(
                    candidate.backend_id.clone(),
                ));
            }
            if !deferred_ids.insert(candidate.backend_id.clone()) {
                return Err(SupportMatrixError::DuplicateDeferredBackend(
                    candidate.backend_id.clone(),
                ));
            }
        }
        Ok(Self {
            schema_version,
            planning_ir_schema_version,
            features: feature_map,
            backends,
            deferred_candidates,
        })
    }

    /// Loads the checked-in generated constants. Production has no backend columns in Phase 02.
    ///
    /// # Errors
    ///
    /// Returns an error if any checked-in generated feature, backend, or deferred-candidate
    /// constant cannot be parsed into a valid capability matrix.
    pub fn generated() -> Result<Self, SupportMatrixError> {
        let features = SUPPORT_FEATURES
            .iter()
            .map(|(id, category, gate)| {
                Ok(SupportFeature {
                    id: SupportFeatureId::new(*id)?,
                    category: parse_category(category)?,
                    gate: parse_gate(gate),
                })
            })
            .collect::<Result<Vec<_>, SupportMatrixError>>()?;
        let mut columns = Vec::new();
        for (id, version, adapter_version) in PRODUCTION_BACKENDS {
            columns.push(BackendSupportColumn {
                backend_id: BackendId::new(id)
                    .map_err(|_| SupportMatrixError::InvalidBackendId((*id).to_owned()))?,
                backend_version: (*version).to_owned(),
                adapter_version: (*adapter_version).to_owned(),
                cells: Vec::new(),
            });
        }
        let deferred_candidates = DEFERRED_BACKEND_CANDIDATES
            .iter()
            .map(|(id, version, phase)| {
                Ok(DeferredBackendCandidate {
                    backend_id: BackendId::new(id)
                        .map_err(|_| SupportMatrixError::InvalidBackendId((*id).to_owned()))?,
                    candidate_version: (*version).to_owned(),
                    owning_phase: *phase,
                })
            })
            .collect::<Result<Vec<_>, SupportMatrixError>>()?;
        Self::new(
            SUPPORT_MATRIX_SCHEMA_VERSION,
            SUPPORT_MATRIX_IR_SCHEMA_VERSION,
            features,
            columns,
            deferred_candidates,
        )
    }

    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub const fn planning_ir_schema_version(&self) -> u32 {
        self.planning_ir_schema_version
    }

    #[must_use]
    pub fn features(&self) -> impl ExactSizeIterator<Item = &SupportFeature> {
        self.features.values()
    }

    #[must_use]
    pub fn production_backend_ids(&self) -> impl ExactSizeIterator<Item = &BackendId> {
        self.backends.keys()
    }

    /// Returns owned, complete columns in stable backend and feature order.
    ///
    /// The returned values are detached from the matrix's validated private maps, so callers
    /// cannot mutate executable support authority.
    #[must_use]
    pub fn backend_columns(&self) -> impl ExactSizeIterator<Item = BackendSupportColumn> + '_ {
        self.backends
            .iter()
            .map(|(backend_id, column)| BackendSupportColumn {
                backend_id: backend_id.clone(),
                backend_version: column.backend_version.clone(),
                adapter_version: column.adapter_version.clone(),
                cells: column
                    .cells
                    .iter()
                    .map(|(feature_id, cell)| (feature_id.clone(), cell.clone()))
                    .collect(),
            })
    }

    #[must_use]
    pub fn deferred_candidates(&self) -> &[DeferredBackendCandidate] {
        &self.deferred_candidates
    }

    pub fn report(
        &self,
        backend: &BackendId,
        requirements: impl IntoIterator<Item = RequiredFeature>,
        estimated_translation_cost: Option<ModelCostEstimate>,
    ) -> CompatibilityReport {
        let column = self.backends.get(backend);
        let mut unsupported_features = Vec::new();
        let mut warnings = Vec::new();
        for required in requirements {
            match column.and_then(|value| value.cells.get(&required.id)) {
                Some(SupportCell::Supported { .. }) => {}
                Some(SupportCell::Degraded {
                    restriction_id,
                    reason,
                    remediation,
                    ..
                }) => warnings.push(CompatibilityWarning {
                    feature_id: required.id,
                    usage_count: required.usage_count,
                    path: required.path,
                    restriction_id: restriction_id.clone(),
                    reason: reason.clone(),
                    remediation: remediation.clone(),
                }),
                Some(SupportCell::Unsupported {
                    reason,
                    remediation,
                    ..
                }) => unsupported_features.push(UnsupportedFeature {
                    feature_id: required.id,
                    usage_count: required.usage_count,
                    path: required.path,
                    reason: reason.clone(),
                    remediation: remediation.clone(),
                }),
                None => unsupported_features.push(UnsupportedFeature {
                    feature_id: required.id,
                    usage_count: required.usage_count,
                    path: required.path,
                    reason: "The backend has no support-matrix claim for this feature.".to_owned(),
                    remediation: "Choose a backend with a tested support claim.".to_owned(),
                }),
            }
        }
        unsupported_features.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
        warnings.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
        let level = if !unsupported_features.is_empty() {
            CompatibilityLevel::Unsupported
        } else if !warnings.is_empty() {
            CompatibilityLevel::Degraded
        } else {
            CompatibilityLevel::Supported
        };
        CompatibilityReport {
            level,
            unsupported_features,
            warnings,
            estimated_translation_cost,
        }
    }

    /// Validates a solver descriptor against its authoritative backend column.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend is absent or its versions or capability declarations
    /// disagree with the matrix.
    pub fn validate_descriptor(
        &self,
        descriptor: &SolverDescriptor,
    ) -> Result<(), SupportMatrixError> {
        let column = self
            .backends
            .get(&descriptor.id)
            .ok_or_else(|| SupportMatrixError::BackendMissing(descriptor.id.clone()))?;
        if descriptor.version != column.backend_version
            || descriptor.adapter_version != column.adapter_version
        {
            return Err(SupportMatrixError::DescriptorVersionMismatch(
                descriptor.id.clone(),
            ));
        }
        let supported = column
            .cells
            .iter()
            .filter_map(|(id, cell)| {
                matches!(cell, SupportCell::Supported { .. }).then_some(id.clone())
            })
            .collect();
        let degraded = column
            .cells
            .iter()
            .filter_map(|(id, cell)| {
                matches!(cell, SupportCell::Degraded { .. }).then_some(id.clone())
            })
            .collect();
        if descriptor.capabilities
            != (SolverCapabilities {
                supported,
                degraded,
            })
        {
            return Err(SupportMatrixError::DescriptorCapabilitiesMismatch(
                descriptor.id.clone(),
            ));
        }
        Ok(())
    }
}

fn parse_category(value: &str) -> Result<SupportFeatureCategory, SupportMatrixError> {
    match value {
        "primitive" => Ok(SupportFeatureCategory::Primitive),
        "objective" => Ok(SupportFeatureCategory::Objective),
        "projection" => Ok(SupportFeatureCategory::Projection),
        "solve" => Ok(SupportFeatureCategory::Solve),
        _ => Err(SupportMatrixError::UnknownGeneratedCategory(
            value.to_owned(),
        )),
    }
}

fn parse_gate(value: &str) -> SupportFeatureGate {
    if value == "unconditional" {
        SupportFeatureGate::Unconditional
    } else {
        SupportFeatureGate::Enabled(value.to_owned())
    }
}

/// Generated or runtime support-matrix contract failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SupportMatrixError {
    UnsupportedMatrixVersion(u32),
    PlanningIrVersionMismatch(u32),
    InvalidFeatureId(String),
    InvalidBackendId(String),
    UnknownGeneratedCategory(String),
    InvalidBackendVersion(BackendId),
    InvalidAdapterVersion(BackendId),
    DuplicateFeature(SupportFeatureId),
    DuplicateCell {
        backend: BackendId,
        feature: SupportFeatureId,
    },
    InvalidCell(SupportFeatureId),
    IncompleteColumn(BackendId),
    DuplicateBackend(BackendId),
    DeferredBackendClaimed(BackendId),
    DuplicateDeferredBackend(BackendId),
    BackendMissing(BackendId),
    DescriptorVersionMismatch(BackendId),
    DescriptorCapabilitiesMismatch(BackendId),
}

impl fmt::Display for SupportMatrixError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "solver support matrix error: {self:?}")
    }
}

impl std::error::Error for SupportMatrixError {}
