use eutheto_types::{DurationMillis, SolveMode};
use serde::{Deserialize, Serialize};

/// Schema version of the deterministic Phase-02 routing profiles.
pub const ROUTING_PROFILE_VERSION: u32 = 1;

/// Stable policy attached to a user-facing effort mode.
///
/// A profile is a bounded search policy, not an optimality or quality promise.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingProfile {
    pub version: u32,
    pub mode: SolveMode,
    pub backend_cap_milliseconds: u64,
    pub allow_fallback: bool,
}

impl RoutingProfile {
    pub const QUICK_V1: Self = Self {
        version: ROUTING_PROFILE_VERSION,
        mode: SolveMode::Quick,
        backend_cap_milliseconds: 1_000,
        allow_fallback: true,
    };

    pub const BALANCED_V1: Self = Self {
        version: ROUTING_PROFILE_VERSION,
        mode: SolveMode::Balanced,
        backend_cap_milliseconds: 3_000,
        allow_fallback: true,
    };

    pub const DEEP_V1: Self = Self {
        version: ROUTING_PROFILE_VERSION,
        mode: SolveMode::Deep,
        backend_cap_milliseconds: 30_000,
        allow_fallback: true,
    };

    #[must_use]
    pub const fn for_mode(mode: SolveMode) -> Option<Self> {
        match mode {
            SolveMode::Quick => Some(Self::QUICK_V1),
            SolveMode::Balanced => Some(Self::BALANCED_V1),
            SolveMode::Deep => Some(Self::DEEP_V1),
            SolveMode::Custom => None,
        }
    }

    #[must_use]
    pub fn backend_cap(self) -> DurationMillis {
        match DurationMillis::new(self.backend_cap_milliseconds) {
            Ok(value) => value,
            Err(_) => DurationMillis::MAX,
        }
    }
}
