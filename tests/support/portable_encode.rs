use eutheto_export::ExportError;
use eutheto_types::{PortableDomainDocument, ScenarioDocument};

/// Archive fixtures use the real synthetic pack rather than a second wire encoder.
pub fn encode_fixture_domain(
    document: &ScenarioDocument,
) -> Result<PortableDomainDocument, ExportError> {
    let registry = eutheto_command::official_registry()
        .map_err(|error| ExportError::InvalidModel(error.to_string()))?;
    registry
        .require(&document.domain_pack.id)
        .and_then(|pack| pack.export_portable(document))
        .map_err(|error| ExportError::InvalidModel(error.to_string()))
}
