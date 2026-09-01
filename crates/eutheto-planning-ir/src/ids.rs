//! Stable planning identifiers, deliberately distinct from application/domain IDs.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Maximum canonical planning identifier size in bytes.
pub const MAX_PLANNING_ID_BYTES: usize = 160;

/// Invalid planning identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanningIdError;

impl fmt::Display for PlanningIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("planning ID must contain at least two lowercase ASCII namespace segments, use only letters, digits, '_' or '-', and be at most 160 bytes")
    }
}

impl std::error::Error for PlanningIdError {}

fn valid(value: &str) -> bool {
    value.len() <= MAX_PLANNING_ID_BYTES
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

macro_rules! planning_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Validates and creates an ID.
            ///
            /// # Errors
            /// Returns [`PlanningIdError`] for noncanonical input.
            pub fn new(value: impl Into<String>) -> Result<Self, PlanningIdError> {
                let value = value.into();
                if valid(&value) {
                    Ok(Self(value))
                } else {
                    Err(PlanningIdError)
                }
            }

            /// Returns the canonical ID string.
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
            type Err = PlanningIdError;

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

planning_id!(
    BoolVariableId,
    "Stable identity of a Boolean planning variable."
);
planning_id!(
    IntVariableId,
    "Stable identity of an integer planning variable."
);
planning_id!(
    IntervalVariableId,
    "Stable identity of an interval planning variable."
);
planning_id!(
    PlanningConstraintId,
    "Stable identity of a planning constraint."
);
planning_id!(
    ObjectiveLevelId,
    "Stable identity of an ordered objective level."
);
planning_id!(ObjectiveTermId, "Stable identity of an objective term.");
planning_id!(AssumptionId, "Stable identity of an assumption.");
planning_id!(ProjectionId, "Stable identity of a solution projection.");
planning_id!(ProvenanceId, "Stable identity of a provenance record.");
planning_id!(ConstraintTag, "Stable identity of a constraint tag.");
planning_id!(
    CapabilityId,
    "Stable identity of a declared capability extension."
);
planning_id!(CompilerId, "Stable identity of a domain compiler.");
planning_id!(MetadataKey, "Stable identity of compile metadata.");
planning_id!(ComponentId, "Stable identity of a mathematical component.");
