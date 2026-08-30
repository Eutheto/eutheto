use eutheto_core::{
    AppCommand, AppCommandResult, AppDependencies, AppPaths, AppQuery, AppQueryResult, EuthetoApp,
};
use eutheto_export::{
    ApplicationMetadata, BackupSections, PortableScenario, ScenarioExportSnapshot,
    assemble_scenario_export,
};
use eutheto_import::{
    CollisionPlan, ImportOptions, InspectionPolicy, MigrationRegistries, RestoreMode,
    inspect_bundle,
};
use eutheto_types::{
    AppError, BundleId, FixedClock, RequestId, Revision, Rfc3339Timestamp, ScenarioDocument,
    SystemIdGenerator,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::sync::Arc;
use tempfile::TempDir;

const OFFICIAL_SCENARIO: &str =
    include_str!("../../../tests/integration/fixtures/official_test_scenario_v1.json");
const EXPECTED_ROUNDTRIP: &str =
    include_str!("../../../tests/integration/fixtures/expected_roundtrip.json");

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn app_result<T>(result: Result<T, AppError>) -> TestResult<T> {
    result.map_err(|error| Box::<dyn Error>::from(std::io::Error::other(format!("{error:?}"))))
}

fn required_str<'a>(value: &'a Value, field: &str) -> TestResult<&'a str> {
    value.get(field).and_then(Value::as_str).ok_or_else(|| {
        Box::<dyn Error>::from(std::io::Error::other(format!(
            "missing string field {field}"
        )))
    })
}

fn required_u64(value: &Value, field: &str) -> TestResult<u64> {
    value.get(field).and_then(Value::as_u64).ok_or_else(|| {
        Box::<dyn Error>::from(std::io::Error::other(format!(
            "missing integer field {field}"
        )))
    })
}

fn fixture(source: &str, expected_format: &str) -> TestResult<Value> {
    let wrapper: Value = serde_json::from_str(source)?;
    assert_eq!(required_str(&wrapper, "format")?, expected_format);
    assert_eq!(required_u64(&wrapper, "schemaVersion")?, 1);
    assert_eq!(required_u64(&wrapper, "version")?, 1);
    let _: BundleId = required_str(&wrapper, "id")?.parse()?;
    Ok(wrapper)
}

fn dependencies(directory: &TempDir) -> TestResult<AppDependencies> {
    Ok(AppDependencies {
        paths: AppPaths {
            database: directory.path().join("eutheto.sqlite"),
            safety_backups: directory.path().join("backups"),
        },
        clock: Arc::new(FixedClock::new(
            "2026-08-29T00:00:00Z".parse::<Rfc3339Timestamp>()?,
        )),
        ids: Arc::new(SystemIdGenerator),
        cancellation: eutheto_export::CancellationSignal::default(),
    })
}

async fn export_scenario(
    app: &EuthetoApp,
    scenario_id: eutheto_types::ScenarioId,
) -> TestResult<Vec<u8>> {
    match app_result(app.query(AppQuery::ExportScenario(scenario_id)).await)? {
        AppQueryResult::Bundle { bytes, .. } => Ok(bytes),
        other => Err(Box::<dyn Error>::from(std::io::Error::other(format!(
            "unexpected export result: {other:?}"
        )))),
    }
}

fn assert_expected_meaning(
    scenario: &PortableScenario,
    expected_meaning: &Value,
    expected_document: &ScenarioDocument,
) -> TestResult {
    assert_eq!(
        scenario.document.scenario_id.to_string(),
        required_str(expected_meaning, "scenarioId")?
    );
    assert_eq!(
        scenario.revision.value(),
        required_u64(expected_meaning, "revision")?
    );
    assert_eq!(&scenario.document, expected_document);
    assert_eq!(
        serde_json::to_value(&scenario.document.metadata)?,
        expected_meaning["metadata"]
    );
    assert_eq!(
        serde_json::to_value(&scenario.document.settings)?,
        expected_meaning["settings"]
    );
    assert_eq!(
        serde_json::to_value(&scenario.document.domain)?,
        expected_meaning["domain"]
    );
    assert_eq!(
        serde_json::to_value(&scenario.document.extensions)?,
        expected_meaning["nonsemanticExtensions"]
    );
    Ok(())
}

fn asserted_fixture_contracts() -> TestResult<(Value, Value)> {
    let official = fixture(OFFICIAL_SCENARIO, "eutheto.test/scenario-document-fixture")?;
    assert_eq!(
        official.get("expectedOutcome"),
        Some(&json!({
            "status": "accepted",
            "scenarioFormatVersion": 1,
            "domainPackId": "official.test",
            "domainPackSchemaVersion": 1,
            "stableScenarioId": "018f47f2-e880-7000-8000-000000000001",
            "preservedNonsemanticExtensions": ["vendor.example"]
        }))
    );
    let expected = fixture(
        EXPECTED_ROUNDTRIP,
        "eutheto.test/semantic-roundtrip-expectation",
    )?;
    assert_eq!(
        required_str(&expected, "sourceFixture")?,
        "official_test_scenario_v1.json"
    );
    assert_eq!(
        required_str(&expected, "portableFixture")?,
        "../../migration/fixtures/portable_v1_scenario.json"
    );
    assert_eq!(
        expected.get("inputContract"),
        Some(&json!({
            "format": "eutheto/scenario",
            "scenarioFormatVersion": 1,
            "portableSchemaVersion": 1,
            "domainPackId": "official.test",
            "domainPackSchemaVersion": 1
        }))
    );
    assert_eq!(
        expected.get("comparison"),
        Some(&json!({
            "document": "exact-semantic-value",
            "revision": "exact",
            "nonsemanticExtensions": "exact-json-value",
            "containerSha256": "excluded-from-semantic-comparison"
        }))
    );
    assert_eq!(
        expected.get("expectedOutcome"),
        Some(&json!({
            "status": "accepted",
            "semanticRoundtrip": true,
            "revisionPreserved": true,
            "nonsemanticExtensionPreserved": true,
            "containerHashRequiredToMatch": false
        }))
    );
    Ok((official, expected))
}

fn portable_fixture(
    official: &Value,
    expected_meaning: &Value,
) -> TestResult<(ScenarioDocument, PortableScenario)> {
    let document: ScenarioDocument = serde_json::from_value(
        official
            .get("input")
            .cloned()
            .ok_or_else(|| std::io::Error::other("official fixture has no input"))?,
    )?;
    assert_eq!(
        document.scenario_id.to_string(),
        required_str(expected_meaning, "scenarioId")?
    );
    assert_eq!(document.domain_pack.id.as_str(), "official.test");
    assert_eq!(document.domain_pack.schema_version, 1);
    assert_eq!(
        document.extensions,
        BTreeMap::from([(
            "vendor.example".to_owned(),
            json!({"opaque": [1, "two", true]})
        )])
    );
    let portable = PortableScenario::current(
        Revision::new(required_u64(expected_meaning, "revision")?),
        document.clone(),
        BTreeSet::new(),
    );
    assert_expected_meaning(&portable, expected_meaning, &document)?;
    Ok((document, portable))
}

fn source_bundle(portable: PortableScenario) -> TestResult<Vec<u8>> {
    Ok(assemble_scenario_export(&ScenarioExportSnapshot {
        bundle_id: "018f47f2-e880-7000-8000-000000000302".parse::<BundleId>()?,
        created_at: "2026-08-29T00:00:00Z".to_owned(),
        application: ApplicationMetadata {
            name: "Eutheto".to_owned(),
            version: "0.1.0".to_owned(),
        },
        title: "Deterministic fixture".to_owned(),
        scenario: portable,
        scenario_revisions: Vec::new(),
        sections: BackupSections::default(),
        nonsemantic_extensions: BTreeSet::new(),
        manifest_extensions: BTreeMap::new(),
    })?)
}

async fn import_fixture(
    app: &EuthetoApp,
    source_bundle: Vec<u8>,
    scenario_id: eutheto_types::ScenarioId,
) -> TestResult {
    let preview_id = match app_result(
        app.query(AppQuery::PreviewImport {
            bytes: source_bundle,
            options: ImportOptions {
                restore_mode: RestoreMode::ImportScenario,
                include_results: false,
                include_assets: false,
            },
        })
        .await,
    )? {
        AppQueryResult::PortablePreview { preview_id, .. } => preview_id,
        other => {
            return Err(Box::<dyn Error>::from(std::io::Error::other(format!(
                "unexpected preview result: {other:?}"
            ))));
        }
    };
    let applied = app_result(
        app.execute(AppCommand::ApplyImport {
            request_id: RequestId::new(&SystemIdGenerator)?,
            preview_id,
            collision_plan: CollisionPlan::default(),
        })
        .await,
    )?;
    assert!(matches!(
        applied,
        AppCommandResult::PortableApplied { ref scenario_ids }
            if scenario_ids == &[scenario_id]
    ));
    Ok(())
}

async fn assert_exported_fixture(
    app: &EuthetoApp,
    document: &ScenarioDocument,
    expected_meaning: &Value,
) -> TestResult {
    let exported = export_scenario(app, document.scenario_id).await?;
    let inspected = inspect_bundle(
        &exported,
        &InspectionPolicy::default(),
        &MigrationRegistries::current_only(),
    )?;
    assert_eq!(inspected.scenarios.len(), 1);
    assert_expected_meaning(&inspected.scenarios[0], expected_meaning, document)?;
    assert_eq!(
        inspected.manifest.nonsemantic_extensions,
        ["vendor.example".to_owned()].into_iter().collect()
    );
    Ok(())
}

#[tokio::test]
async fn official_fixture_roundtrips_through_application_storage_and_reopen() -> TestResult {
    let (official, expected) = asserted_fixture_contracts()?;
    let expected_meaning = expected
        .get("expectedMeaning")
        .ok_or_else(|| std::io::Error::other("roundtrip fixture has no expected meaning"))?;
    let (document, portable) = portable_fixture(&official, expected_meaning)?;
    let source_bundle = source_bundle(portable)?;

    let directory = tempfile::tempdir()?;
    let app = app_result(EuthetoApp::open(dependencies(&directory)?).await)?;
    import_fixture(&app, source_bundle, document.scenario_id).await?;
    assert_exported_fixture(&app, &document, expected_meaning).await?;
    drop(app);

    let reopened = app_result(EuthetoApp::open(dependencies(&directory)?).await)?;
    assert_exported_fixture(&reopened, &document, expected_meaning).await
}
