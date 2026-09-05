use eutheto_command::{OFFICIAL_TEST_PACK_ID, OfficialTestPack};
use eutheto_core::{
    AppCommand, AppCommandResult, AppDependencies, AppPaths, AppQuery, AppQueryResult, EuthetoApp,
    ProjectScope,
};
use eutheto_domain_api::{
    CompileContext, CounterfactualCompileContext, DomainBatchCommand, DomainCatalog,
    DomainMutation, DomainPack, DomainPackDescriptor, DomainPackError, DomainPackRegistry,
    DomainShareResult, DomainValidationReport, HistoricalPortableDomainDocument,
    PortableDomainDocument, PortableImportContext, ShareResultOptions,
};
use eutheto_domain_ir::{
    AcceptedResult, CounterfactualConditionV1, EvidenceRenderRequestV1, EvidenceRenderResultV1,
    ExplanationCapability, NormalizedSolution, ScoreVector, VerificationContextV1,
    VerificationReport, VerificationScope,
};
use eutheto_export::{
    ApplicationMetadata, BackupSections, PortableScenario, ScenarioExportSnapshot,
    assemble_scenario_export,
};
use eutheto_import::{
    CollisionPlan, ImportOptions, InspectionPolicy, MigrationRegistries, RestoreMode,
    inspect_bundle,
};
use eutheto_planning_ir::{CandidateValues, PlanningProblem};
use eutheto_store::SqliteScenarioStore;
use eutheto_types::{
    AppError, BundleId, EntityId, FixedClock, RequestId, Revision, Rfc3339Timestamp,
    ScenarioDocument, SolutionId, SystemIdGenerator, ValidationIssue, ValidationSeverity,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tempfile::TempDir;

const OFFICIAL_SCENARIO: &str =
    include_str!("../../../tests/integration/fixtures/official_test_scenario_v1.json");
const EXPECTED_ROUNDTRIP: &str =
    include_str!("../../../tests/integration/fixtures/expected_roundtrip.json");

#[derive(Default)]
struct PackCounters {
    initializations: AtomicUsize,
    migrations: AtomicUsize,
    validations: AtomicUsize,
}

#[derive(Clone)]
struct HistoricalFixturePack {
    counters: Arc<PackCounters>,
    fail_on_version: Option<u32>,
    invalid_initialization: bool,
}

impl HistoricalFixturePack {
    fn unsupported<T>() -> Result<T, DomainPackError> {
        Err(DomainPackError::Contract(
            "the fixture pack implements only initialization and import migration".to_owned(),
        ))
    }

    fn validation_report(document: &ScenarioDocument) -> DomainValidationReport {
        let valid = document.domain_pack.schema_version == 3
            && document
                .domain
                .entities
                .values()
                .all(|entity| entity.get("migrationSteps") == Some(&json!([2, 3])));
        DomainValidationReport {
            issues: if valid {
                Vec::new()
            } else {
                vec![ValidationIssue {
                    code: "fixture.migration_invalid".to_owned(),
                    severity: ValidationSeverity::Error,
                    message: "historical migration did not reach the deterministic current form"
                        .to_owned(),
                    field_path: Some("/domain".to_owned()),
                    resource: None,
                }]
            },
        }
    }
}

impl DomainPack for HistoricalFixturePack {
    fn descriptor(&self) -> Result<DomainPackDescriptor, DomainPackError> {
        let mut descriptor = OfficialTestPack.descriptor()?;
        descriptor.scenario_versions.latest = 3;
        descriptor.scenario_versions.migratable_from = [1, 2].into_iter().collect();
        descriptor.explanation_capabilities.clear();
        Ok(descriptor)
    }

    fn catalog(&self) -> Result<DomainCatalog, DomainPackError> {
        let mut catalog = OfficialTestPack.catalog()?;
        catalog.scenario_schema_version = 3;
        Ok(catalog)
    }

    fn new_document(
        &self,
        mut shell: ScenarioDocument,
    ) -> Result<ScenarioDocument, DomainPackError> {
        self.counters.initializations.fetch_add(1, Ordering::SeqCst);
        if self.invalid_initialization {
            shell.domain_pack.schema_version = 2;
            return Ok(shell);
        }
        let person_id = "018f47f2-e880-7000-8000-000000000099"
            .parse::<EntityId>()
            .map_err(|error| DomainPackError::Contract(error.to_string()))?;
        shell.domain.entities.insert(
            person_id,
            json!({
                "id": person_id,
                "initializerDefault": true,
                "migrationSteps": [2, 3]
            }),
        );
        Ok(shell)
    }

    fn migrate_document(
        &self,
        mut document: ScenarioDocument,
    ) -> Result<ScenarioDocument, DomainPackError> {
        let source_version = document.domain_pack.schema_version;
        self.counters.migrations.fetch_add(1, Ordering::SeqCst);
        if self.fail_on_version == Some(source_version) {
            return Err(DomainPackError::Contract(
                "fixture-requested migration failure".to_owned(),
            ));
        }
        let target_version = source_version
            .checked_add(1)
            .ok_or(DomainPackError::UnsupportedVersion(source_version))?;
        if document.domain_pack.id.as_str() != OFFICIAL_TEST_PACK_ID
            || !matches!(source_version, 1 | 2)
        {
            return Err(DomainPackError::UnsupportedVersion(source_version));
        }
        for entity in document.domain.entities.values_mut() {
            let object = entity.as_object_mut().ok_or_else(|| {
                DomainPackError::Contract("fixture entity must be an object".to_owned())
            })?;
            let steps = object
                .entry("migrationSteps")
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .ok_or_else(|| {
                    DomainPackError::Contract("fixture migration trail must be an array".to_owned())
                })?;
            steps.push(Value::from(target_version));
        }
        document.domain_pack.schema_version = target_version;
        Ok(document)
    }

    fn validate_fast(&self, document: &ScenarioDocument) -> DomainValidationReport {
        Self::validation_report(document)
    }

    fn validate_full(&self, document: &ScenarioDocument) -> DomainValidationReport {
        self.counters.validations.fetch_add(1, Ordering::SeqCst);
        Self::validation_report(document)
    }

    fn apply_batch(
        &self,
        _document: &ScenarioDocument,
        _batch: &DomainBatchCommand,
    ) -> Result<DomainMutation, DomainPackError> {
        Self::unsupported()
    }

    fn compile(
        &self,
        _document: &ScenarioDocument,
        _context: &CompileContext,
    ) -> Result<PlanningProblem, DomainPackError> {
        Self::unsupported()
    }

    fn project(
        &self,
        _problem: &PlanningProblem,
        _candidate: &CandidateValues,
        _solution_id: SolutionId,
    ) -> Result<NormalizedSolution, DomainPackError> {
        Self::unsupported()
    }

    fn verification_scope(
        &self,
        _document: &ScenarioDocument,
        _scenario_revision: u64,
    ) -> Result<VerificationScope, DomainPackError> {
        Self::unsupported()
    }

    fn verify(
        &self,
        _document: &ScenarioDocument,
        _solution: &NormalizedSolution,
        _context: &VerificationContextV1,
        _authoritative_score: &ScoreVector,
    ) -> Result<VerificationReport, DomainPackError> {
        Self::unsupported()
    }

    fn score(
        &self,
        _document: &ScenarioDocument,
        _solution: &NormalizedSolution,
    ) -> Result<ScoreVector, DomainPackError> {
        Self::unsupported()
    }

    fn export_portable(
        &self,
        _document: &ScenarioDocument,
    ) -> Result<PortableDomainDocument, DomainPackError> {
        Self::unsupported()
    }

    fn migrate_portable(
        &self,
        _document: HistoricalPortableDomainDocument,
    ) -> Result<PortableDomainDocument, DomainPackError> {
        Self::unsupported()
    }

    fn import_portable(
        &self,
        _document: &PortableDomainDocument,
        _context: &PortableImportContext,
    ) -> Result<ScenarioDocument, DomainPackError> {
        Self::unsupported()
    }

    fn build_share_result(
        &self,
        _document: &ScenarioDocument,
        _accepted: &AcceptedResult,
        _options: ShareResultOptions,
    ) -> Result<DomainShareResult, DomainPackError> {
        Self::unsupported()
    }

    fn build_view(
        &self,
        _document: &ScenarioDocument,
        _solution: Option<&NormalizedSolution>,
        _view_id: &str,
    ) -> Result<eutheto_domain_api::DomainView, DomainPackError> {
        Self::unsupported()
    }

    fn render_evidence(
        &self,
        _document: &ScenarioDocument,
        request: &EvidenceRenderRequestV1,
    ) -> Result<EvidenceRenderResultV1, DomainPackError> {
        Err(DomainPackError::UnsupportedExplanationCapability(
            explanation_capability(request.kind),
        ))
    }

    fn compile_counterfactual(
        &self,
        _document: &ScenarioDocument,
        _condition: &CounterfactualConditionV1,
        _context: &CounterfactualCompileContext<'_>,
    ) -> Result<PlanningProblem, DomainPackError> {
        Err(DomainPackError::UnsupportedExplanationCapability(
            ExplanationCapability::Counterfactual,
        ))
    }
}
fn explanation_capability(kind: eutheto_domain_ir::ExplanationKind) -> ExplanationCapability {
    match kind {
        eutheto_domain_ir::ExplanationKind::Validation => ExplanationCapability::Validation,
        eutheto_domain_ir::ExplanationKind::Infeasibility => ExplanationCapability::Infeasibility,
        eutheto_domain_ir::ExplanationKind::Assignment => ExplanationCapability::Assignment,
        eutheto_domain_ir::ExplanationKind::Counterfactual => ExplanationCapability::Counterfactual,
        eutheto_domain_ir::ExplanationKind::SolutionDifference => {
            ExplanationCapability::SolutionDifference
        }
        eutheto_domain_ir::ExplanationKind::Repair => ExplanationCapability::Repair,
        eutheto_domain_ir::ExplanationKind::OptimalityStatus => {
            ExplanationCapability::OptimalityStatus
        }
    }
}

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn app_result<T>(result: Result<T, AppError>) -> TestResult<T> {
    result.map_err(|error| Box::<dyn Error>::from(std::io::Error::other(format!("{error:?}"))))
}
fn private_tempdir() -> TestResult<TempDir> {
    let base = dirs::home_dir().ok_or("platform home directory is unavailable")?;
    std::fs::create_dir_all(&base)?;
    Ok(tempfile::Builder::new()
        .prefix("eutheto-core-fixture-test-")
        .tempdir_in(base)?)
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
        monotonic_clock: Arc::new(eutheto_types::FixedMonotonicClock::default()),
        ids: Arc::new(SystemIdGenerator),
        cancellation: eutheto_types::CancellationToken::default(),
    })
}

async fn fixture_app(
    directory: &TempDir,
    counters: Arc<PackCounters>,
    fail_on_version: Option<u32>,
) -> TestResult<EuthetoApp> {
    fixture_app_with_initializer(directory, counters, fail_on_version, false).await
}

async fn fixture_app_with_initializer(
    directory: &TempDir,
    counters: Arc<PackCounters>,
    fail_on_version: Option<u32>,
    invalid_initialization: bool,
) -> TestResult<EuthetoApp> {
    let dependencies = dependencies(directory)?;
    let (store, initialization) = SqliteScenarioStore::open(&dependencies.paths.database).await?;
    let registry = DomainPackRegistry::builder()
        .register(HistoricalFixturePack {
            counters,
            fail_on_version,
            invalid_initialization,
        })
        .build()?;
    app_result(EuthetoApp::from_initialized_store_with_pack_registry(
        Arc::new(store),
        initialization,
        dependencies,
        registry,
    ))
}

fn official_document() -> TestResult<ScenarioDocument> {
    let official = fixture(OFFICIAL_SCENARIO, "eutheto.test/scenario-document-fixture")?;
    Ok(serde_json::from_value(
        official
            .get("input")
            .cloned()
            .ok_or_else(|| std::io::Error::other("official fixture has no input"))?,
    )?)
}

fn document_at_pack_version(version: u32) -> TestResult<ScenarioDocument> {
    let mut document = official_document()?;
    document.domain_pack.schema_version = version;
    Ok(document)
}

fn deterministically_migrated_document() -> TestResult<ScenarioDocument> {
    let mut document = document_at_pack_version(3)?;
    for entity in document.domain.entities.values_mut() {
        entity
            .as_object_mut()
            .ok_or_else(|| std::io::Error::other("fixture entity must be an object"))?
            .insert("migrationSteps".to_owned(), json!([2, 3]));
    }
    Ok(document)
}

async fn stored_document(
    app: &EuthetoApp,
    scenario_id: eutheto_types::ScenarioId,
) -> TestResult<ScenarioDocument> {
    match app_result(app.query(AppQuery::OpenProject(scenario_id)).await)? {
        AppQueryResult::Scenario(scenario) => Ok(scenario.document),
        other => Err(std::io::Error::other(format!(
            "unexpected stored scenario result: {other:?}"
        ))
        .into()),
    }
}

async fn project_count(app: &EuthetoApp) -> TestResult<usize> {
    match app_result(app.query(AppQuery::ListProjects(ProjectScope::All)).await)? {
        AppQueryResult::Projects(projects) => Ok(projects.len()),
        other => {
            Err(std::io::Error::other(format!("unexpected project list result: {other:?}")).into())
        }
    }
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
    source_bundle_with_revisions(portable, Vec::new())
}

fn source_bundle_with_revisions(
    portable: PortableScenario,
    scenario_revisions: Vec<PortableScenario>,
) -> TestResult<Vec<u8>> {
    Ok(assemble_scenario_export(&ScenarioExportSnapshot {
        bundle_id: "018f47f2-e880-7000-8000-000000000302".parse::<BundleId>()?,
        created_at: "2026-08-29T00:00:00Z".to_owned(),
        application: ApplicationMetadata {
            name: "Eutheto".to_owned(),
            version: "0.1.0".to_owned(),
        },
        title: "Deterministic fixture".to_owned(),
        scenario: portable,
        scenario_revisions,
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
        AppCommandResult::PortableApplied { ref scenarios }
            if matches!(
                scenarios.as_slice(),
                [applied_scenario]
                    if applied_scenario.source_scenario_id == scenario_id
                        && applied_scenario.scenario_id == scenario_id
            )
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
async fn project_creation_persists_pack_initializer_state_at_revision_zero() -> TestResult {
    let directory = private_tempdir()?;
    let counters = Arc::new(PackCounters::default());
    let app = fixture_app(&directory, Arc::clone(&counters), None).await?;
    let source = official_document()?;
    let mut domain_pack = source.domain_pack;
    domain_pack.schema_version = 3;
    let requested_settings = source.settings;

    let created = app_result(
        app.execute(AppCommand::CreateProject {
            request_id: RequestId::new(&SystemIdGenerator)?,
            title: "Initialized fixture".to_owned(),
            description: "Pack-owned defaults must be stored".to_owned(),
            domain_pack,
            settings: requested_settings.clone(),
        })
        .await,
    )?;
    let metadata = match created {
        AppCommandResult::Project(metadata) => metadata,
        other => {
            return Err(std::io::Error::other(format!(
                "unexpected project creation result: {other:?}"
            ))
            .into());
        }
    };

    assert_eq!(metadata.revision, Revision::INITIAL);
    assert_eq!(metadata.domain_pack.schema_version, 3);
    assert_eq!(counters.initializations.load(Ordering::SeqCst), 1);
    assert_eq!(counters.validations.load(Ordering::SeqCst), 1);
    let stored = stored_document(&app, metadata.scenario_id).await?;
    assert_eq!(stored.settings, requested_settings);
    assert_eq!(stored.metadata.title, "Initialized fixture");
    assert_eq!(
        stored
            .domain
            .entities
            .values()
            .next()
            .and_then(|entity| entity.get("initializerDefault")),
        Some(&Value::Bool(true))
    );
    drop(app);

    let reopened = fixture_app(&directory, Arc::new(PackCounters::default()), None).await?;
    assert_eq!(
        stored_document(&reopened, metadata.scenario_id).await?,
        stored
    );
    Ok(())
}

#[tokio::test]
async fn historical_schema_project_creation_is_rejected_without_initialization_or_persistence()
-> TestResult {
    let directory = private_tempdir()?;
    let counters = Arc::new(PackCounters::default());
    let app = fixture_app(&directory, Arc::clone(&counters), None).await?;
    let source = official_document()?;

    assert!(matches!(
        app.execute(AppCommand::CreateProject {
            request_id: RequestId::new(&SystemIdGenerator)?,
            title: "Historical fixture".to_owned(),
            description: String::new(),
            domain_pack: source.domain_pack,
            settings: source.settings,
        })
        .await,
        Err(AppError::Unsupported(feature))
            if feature.code == "domain_pack.unsupported"
    ));
    assert_eq!(counters.initializations.load(Ordering::SeqCst), 0);
    assert_eq!(counters.validations.load(Ordering::SeqCst), 0);
    assert_eq!(project_count(&app).await?, 0);
    drop(app);

    let reopened = fixture_app(&directory, Arc::new(PackCounters::default()), None).await?;
    assert_eq!(project_count(&reopened).await?, 0);
    Ok(())
}

#[tokio::test]
async fn invalid_pack_initializer_output_is_rejected_before_persistence() -> TestResult {
    let directory = private_tempdir()?;
    let counters = Arc::new(PackCounters::default());
    let app = fixture_app_with_initializer(&directory, Arc::clone(&counters), None, true).await?;
    let source = official_document()?;
    let mut domain_pack = source.domain_pack;
    domain_pack.schema_version = 3;

    assert!(matches!(
        app.execute(AppCommand::CreateProject {
            request_id: RequestId::new(&SystemIdGenerator)?,
            title: "Invalid initializer".to_owned(),
            description: String::new(),
            domain_pack,
            settings: source.settings,
        })
        .await,
        Err(AppError::Protocol(failure))
            if failure.code == "application.pack_initialization_invalid"
    ));
    assert_eq!(counters.initializations.load(Ordering::SeqCst), 1);
    assert_eq!(counters.validations.load(Ordering::SeqCst), 0);
    assert_eq!(project_count(&app).await?, 0);
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

    let directory = private_tempdir()?;
    let app = app_result(EuthetoApp::open(dependencies(&directory)?).await)?;
    import_fixture(&app, source_bundle, document.scenario_id).await?;
    assert_exported_fixture(&app, &document, expected_meaning).await?;
    drop(app);

    let reopened = app_result(EuthetoApp::open(dependencies(&directory)?).await)?;
    assert_exported_fixture(&reopened, &document, expected_meaning).await
}

#[tokio::test]
async fn historical_pack_documents_migrate_deterministically_before_commit_and_reopen() -> TestResult
{
    let source_document = document_at_pack_version(1)?;
    let expected = deterministically_migrated_document()?;
    let source = source_bundle_with_revisions(
        PortableScenario::current(Revision::new(7), source_document.clone(), BTreeSet::new()),
        vec![PortableScenario::current(
            Revision::new(6),
            source_document.clone(),
            BTreeSet::new(),
        )],
    )?;
    let original_metadata = source_document.metadata.clone();
    let original_settings = source_document.settings.clone();
    let original_extensions = source_document.extensions.clone();

    let first_directory = private_tempdir()?;
    let first_counters = Arc::new(PackCounters::default());
    let first = fixture_app(&first_directory, Arc::clone(&first_counters), None).await?;
    import_fixture(&first, source.clone(), source_document.scenario_id).await?;
    assert_eq!(first_counters.migrations.load(Ordering::SeqCst), 4);
    assert_eq!(first_counters.validations.load(Ordering::SeqCst), 2);
    let committed = stored_document(&first, source_document.scenario_id).await?;
    assert_eq!(committed, expected);
    assert_eq!(committed.metadata, original_metadata);
    assert_eq!(committed.settings, original_settings);
    assert_eq!(committed.extensions, original_extensions);
    drop(first);

    let reopened = fixture_app(&first_directory, Arc::clone(&first_counters), None).await?;
    assert_eq!(
        stored_document(&reopened, source_document.scenario_id).await?,
        expected
    );
    assert_eq!(first_counters.migrations.load(Ordering::SeqCst), 4);
    let reopened_export = export_scenario(&reopened, source_document.scenario_id).await?;
    let reopened_inspection = inspect_bundle(
        &reopened_export,
        &InspectionPolicy::default(),
        &MigrationRegistries::current_only(),
    )?;
    assert!(
        reopened_inspection
            .scenario_revisions
            .iter()
            .all(|revision| revision.document == expected)
    );
    drop(reopened);

    let second_directory = private_tempdir()?;
    let second_counters = Arc::new(PackCounters::default());
    let second = fixture_app(&second_directory, Arc::clone(&second_counters), None).await?;
    import_fixture(&second, source, source_document.scenario_id).await?;
    assert_eq!(
        stored_document(&second, source_document.scenario_id).await?,
        expected
    );
    assert_eq!(second_counters.migrations.load(Ordering::SeqCst), 4);
    assert_eq!(second_counters.validations.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn historical_pack_migration_failure_is_atomic() -> TestResult {
    let directory = private_tempdir()?;
    let counters = Arc::new(PackCounters::default());
    let app = fixture_app(&directory, Arc::clone(&counters), Some(2)).await?;
    let document = document_at_pack_version(1)?;
    let source = source_bundle(PortableScenario::current(
        Revision::new(7),
        document,
        BTreeSet::new(),
    ))?;

    assert_eq!(project_count(&app).await?, 0);
    assert!(matches!(
        app.query(AppQuery::PreviewImport {
            bytes: source,
            options: ImportOptions {
                restore_mode: RestoreMode::ImportScenario,
                include_results: false,
                include_assets: false,
            },
        })
        .await,
        Err(AppError::Validation(report))
            if report
                .issues
                .iter()
                .any(|issue| issue.code == "domain_pack.migration_failed")
    ));
    assert_eq!(counters.migrations.load(Ordering::SeqCst), 2);
    assert_eq!(counters.validations.load(Ordering::SeqCst), 0);
    assert_eq!(project_count(&app).await?, 0);
    drop(app);

    let reopened = fixture_app(&directory, Arc::new(PackCounters::default()), None).await?;
    assert_eq!(project_count(&reopened).await?, 0);
    Ok(())
}

#[tokio::test]
async fn current_and_unsupported_pack_documents_never_invoke_migration() -> TestResult {
    let directory = private_tempdir()?;
    let counters = Arc::new(PackCounters::default());
    let app = fixture_app(&directory, Arc::clone(&counters), None).await?;
    let current = document_at_pack_version(3)?;
    let current_source = source_bundle(PortableScenario::current(
        Revision::new(7),
        current.clone(),
        BTreeSet::new(),
    ))?;
    import_fixture(&app, current_source, current.scenario_id).await?;
    assert_eq!(stored_document(&app, current.scenario_id).await?, current);
    assert_eq!(counters.migrations.load(Ordering::SeqCst), 0);
    assert_eq!(counters.validations.load(Ordering::SeqCst), 0);

    let newer = document_at_pack_version(4)?;
    let newer_source = source_bundle(PortableScenario::current(
        Revision::new(8),
        newer,
        BTreeSet::new(),
    ))?;
    assert!(matches!(
        app.query(AppQuery::PreviewImport {
            bytes: newer_source.clone(),
            options: ImportOptions {
                restore_mode: RestoreMode::ImportScenario,
                include_results: false,
                include_assets: false,
            },
        })
        .await,
        Err(AppError::Validation(report))
            if report
                .issues
                .iter()
                .any(|issue| issue.code == "domain_pack.unsupported")
    ));
    assert_eq!(counters.migrations.load(Ordering::SeqCst), 0);
    assert_eq!(project_count(&app).await?, 1);

    let mut missing = document_at_pack_version(3)?;
    missing.domain_pack.id = "missing.pack".parse()?;
    let missing_source = source_bundle(PortableScenario::current(
        Revision::new(9),
        missing,
        BTreeSet::new(),
    ))?;
    assert!(matches!(
        app.query(AppQuery::PreviewImport {
            bytes: missing_source,
            options: ImportOptions {
                restore_mode: RestoreMode::ImportScenario,
                include_results: false,
                include_assets: false,
            },
        })
        .await,
        Err(AppError::Validation(report))
            if report
                .issues
                .iter()
                .any(|issue| issue.code == "domain_pack.unsupported")
    ));
    assert_eq!(counters.migrations.load(Ordering::SeqCst), 0);
    assert_eq!(project_count(&app).await?, 1);

    let preview_id = match app_result(
        app.query(AppQuery::InspectUnopenedBundle {
            bytes: newer_source.clone(),
        })
        .await,
    )? {
        AppQueryResult::UnopenedBundlePreview { preview_id, .. } => preview_id,
        other => {
            return Err(std::io::Error::other(format!(
                "unexpected unopened preview result: {other:?}"
            ))
            .into());
        }
    };
    let destination = directory.path().join("unsupported-preserved.eutheto");
    assert!(matches!(
        app_result(
            app.execute(AppCommand::ExactReexportUnopenedBundle {
                preview_id,
                destination: destination.clone(),
            })
            .await
        )?,
        AppCommandResult::UnopenedBundleReexported
    ));
    assert_eq!(std::fs::read(destination)?, newer_source);
    assert_eq!(counters.migrations.load(Ordering::SeqCst), 0);
    Ok(())
}
