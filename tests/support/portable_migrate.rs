use eutheto_export::{PortableScenario, validate_scenario_snapshot};
use eutheto_import::{MigrationFailure, PortableMigrationOutcome};
use eutheto_types::ScenarioSnapshotV1;
use serde_json::Value;

pub fn migrate_fixture_v1(value: Value) -> Result<PortableMigrationOutcome, MigrationFailure> {
    let snapshot: ScenarioSnapshotV1 = serde_json::from_value(value)
        .map_err(|error| MigrationFailure::Invalid(error.to_string()))?;
    validate_scenario_snapshot(&snapshot)
        .map_err(|error| MigrationFailure::Invalid(error.to_string()))?;
    let domain = super::portable_encode::encode_fixture_domain(&snapshot.document)
        .map_err(|error| MigrationFailure::Invalid(error.to_string()))?;
    let introduced_requirements = domain
        .required_capabilities
        .difference(&snapshot.required_capabilities)
        .cloned()
        .collect();
    let wire = PortableScenario::from_snapshot(&snapshot, domain)
        .map_err(|error| MigrationFailure::Invalid(error.to_string()))?;
    Ok(PortableMigrationOutcome {
        value: serde_json::to_value(wire)
            .map_err(|error| MigrationFailure::Invalid(error.to_string()))?,
        introduced_requirements,
        applied_migrations: Vec::new(),
    })
}
