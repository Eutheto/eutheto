use eutheto_domain_api::{DomainPack, PortableImportContext};
use eutheto_export::PortableScenario;
use eutheto_import::{DecodedPortableDomain, MigrationFailure};
use eutheto_types::{DomainPackRef, ScenarioDocument, ScenarioDomain};
use std::collections::BTreeMap;

/// Current-wire archive fixtures exercise the registered pack's real decoder.
pub fn decode_fixture_domain(
    wire: &PortableScenario,
) -> Result<DecodedPortableDomain, MigrationFailure> {
    let failure =
        |error: eutheto_domain_api::DomainPackError| MigrationFailure::Invalid(error.to_string());
    let registry = eutheto_command::official_registry().map_err(failure)?;
    let pack = registry.require(&wire.domain.pack_id).map_err(failure)?;
    decode_fixture_domain_with_pack(wire, pack)
}

pub fn decode_fixture_domain_with_pack(
    wire: &PortableScenario,
    pack: &dyn DomainPack,
) -> Result<DecodedPortableDomain, MigrationFailure> {
    let failure =
        |error: eutheto_domain_api::DomainPackError| MigrationFailure::Invalid(error.to_string());
    let context = PortableImportContext {
        scenario_shell: ScenarioDocument::new(
            wire.scenario_id,
            DomainPackRef {
                id: wire.domain.pack_id.clone(),
                schema_version: pack.descriptor().map_err(failure)?.scenario_versions.latest,
            },
            wire.metadata.clone(),
            wire.settings.clone(),
            ScenarioDomain::default(),
            BTreeMap::new(),
        ),
    };
    Ok(DecodedPortableDomain {
        document: pack
            .import_portable(&wire.domain, &context)
            .map_err(failure)?,
        required_capabilities: wire.domain.required_capabilities.clone(),
        applied_migrations: Vec::new(),
    })
}
