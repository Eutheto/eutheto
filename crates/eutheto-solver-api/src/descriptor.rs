use crate::SupportFeatureId;
use eutheto_types::BackendId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

const MAX_DISPLAY_NAME_BYTES: usize = 120;
const MAX_VERSION_BYTES: usize = 64;
const MAX_LICENSE_FIELD_BYTES: usize = 256;
const MAX_SOURCE_URL_BYTES: usize = 2_048;

/// How a backend is delivered to the application.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SolverDistribution {
    BuiltIn,
    BundledWorker,
    UserProvided,
}

/// Maturity exposed without implying support beyond the generated matrix.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BackendStability {
    Stable,
    Beta,
    Experimental,
}

/// Bounded license metadata shown before a backend is enabled.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LicenseMetadata {
    pub spdx_expression: String,
    pub license_name: String,
    pub source_url: Option<String>,
}

/// Matrix-derived capability declaration copied into a backend descriptor.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SolverCapabilities {
    pub supported: BTreeSet<SupportFeatureId>,
    pub degraded: BTreeSet<SupportFeatureId>,
}

impl SolverCapabilities {
    /// Features for which the backend can be invoked, including restricted support.
    #[must_use]
    pub fn available(&self) -> BTreeSet<&SupportFeatureId> {
        self.supported.iter().chain(&self.degraded).collect()
    }
}

/// Stable, serializable backend metadata. It contains no executable backend state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SolverDescriptor {
    pub id: BackendId,
    pub display_name: String,
    pub version: String,
    pub adapter_version: String,
    pub distribution: SolverDistribution,
    pub license: LicenseMetadata,
    pub stability: BackendStability,
    pub capabilities: SolverCapabilities,
}

impl SolverDescriptor {
    /// Validates bounded data fields and capability-set consistency.
    ///
    /// Matrix agreement is checked separately by [`crate::CapabilityMatrix`].
    ///
    /// # Errors
    ///
    /// Returns an error when a bounded metadata field is invalid, the optional source URL is not
    /// HTTPS, or a feature is declared both supported and degraded.
    pub fn validate(&self) -> Result<(), DescriptorError> {
        if self.display_name.is_empty()
            || self.display_name.len() > MAX_DISPLAY_NAME_BYTES
            || self.display_name.chars().any(char::is_control)
        {
            return Err(DescriptorError::InvalidDisplayName);
        }
        if !valid_version(&self.version) {
            return Err(DescriptorError::InvalidVersion);
        }
        if !valid_version(&self.adapter_version) {
            return Err(DescriptorError::InvalidAdapterVersion);
        }
        if self.license.spdx_expression.is_empty()
            || self.license.spdx_expression.len() > MAX_LICENSE_FIELD_BYTES
            || self.license.spdx_expression.chars().any(char::is_control)
            || self.license.license_name.is_empty()
            || self.license.license_name.len() > MAX_LICENSE_FIELD_BYTES
            || self.license.license_name.chars().any(char::is_control)
        {
            return Err(DescriptorError::InvalidLicense);
        }
        if self.license.source_url.as_ref().is_some_and(|url| {
            url.len() > MAX_SOURCE_URL_BYTES
                || !url.starts_with("https://")
                || url.chars().any(char::is_control)
        }) {
            return Err(DescriptorError::InvalidSourceUrl);
        }
        if let Some(feature) = self
            .capabilities
            .supported
            .intersection(&self.capabilities.degraded)
            .next()
        {
            return Err(DescriptorError::OverlappingCapability(feature.clone()));
        }
        Ok(())
    }
}

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_VERSION_BYTES
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
}

/// Invalid descriptor data rejected before registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DescriptorError {
    InvalidDisplayName,
    InvalidVersion,
    InvalidAdapterVersion,
    InvalidLicense,
    InvalidSourceUrl,
    OverlappingCapability(SupportFeatureId),
}

impl fmt::Display for DescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid solver descriptor: {self:?}")
    }
}

impl std::error::Error for DescriptorError {}
