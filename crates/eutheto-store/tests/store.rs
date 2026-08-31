use eutheto_export::{
    ApplicationMetadata, PortableProjectMetadata, PortableScenario, SemanticCapability,
    validate_current_portable_scenario,
};
use eutheto_import::{
    ImportProvenance, PreviewBinding, RestoreAuthorization, RestoreMode, SafetyBackupEvidence,
    StagedBackupRestore, StagedDisposition, StagedImport, StagedScenario,
};
#[cfg(debug_assertions)]
use eutheto_store::Failpoint;
use eutheto_store::{
    AppSetting, CommandWrite, JournalWrite, NewProject, OpenOptions, ProjectListScope,
    RedoBranchPolicy, SafetyBackupFailureReceipt, SnapshotPolicy, SqliteScenarioStore,
    StagedLibraryApply, StoreError, StoredProject, ensure_private_application_directory,
};
use eutheto_types::{
    ActorRef, BundleId, CommandId, CommandSource, DomainPackRef, GapPolicy, Horizon, IanaTimeZone,
    LocaleTag, MAX_SCENARIO_DOCUMENT_BYTES, OverlapPolicy, PackId, PersonId, PortableAsset,
    Revision, Rfc3339Timestamp, RuleId, ScenarioDocument, ScenarioDomain, ScenarioId,
    ScenarioMetadata, ScenarioSettings, SolveRunId, SupplementalIdentity, SupplementalSectionKind,
    UnitSystem,
};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tempfile::tempdir;
use uuid::Uuid;

const CREATED: &str = "2026-08-28T23:00:00Z";
const UPDATED: &str = "2026-08-28T23:01:00Z";
const LATER: &str = "2026-08-28T23:02:00Z";

fn scenario_id(suffix: u16) -> Result<ScenarioId, uuid::Error> {
    let text = format!("018f47f2-e880-7000-8000-{suffix:012x}");
    Uuid::parse_str(&text).map(ScenarioId::from_uuid)
}

fn pack_id() -> Result<PackId, eutheto_types::NamespacedIdError> {
    PackId::new("official.test")
}

fn timestamp(value: &str) -> Result<Rfc3339Timestamp, jiff::Error> {
    Rfc3339Timestamp::parse(value)
}

fn document(id: ScenarioId) -> Result<ScenarioDocument, Box<dyn Error>> {
    Ok(ScenarioDocument::new(
        id,
        DomainPackRef {
            id: pack_id()?,
            schema_version: 1,
        },
        ScenarioMetadata {
            title: "Clinic plan".to_owned(),
            description: "A persisted generic scenario".to_owned(),
            created_at: timestamp(CREATED)?,
            updated_at: timestamp(CREATED)?,
        },
        ScenarioSettings {
            time_zone: IanaTimeZone::parse("America/Chicago")?,
            locale: LocaleTag::parse("en-US")?,
            units: UnitSystem::UsCustomary,
            horizon: Horizon::new(
                timestamp("2026-09-01T05:00:00Z")?,
                timestamp("2026-10-01T05:00:00Z")?,
            )?,
            gap_policy: GapPolicy::Reject,
            overlap_policy: OverlapPolicy::Earlier,
        },
        ScenarioDomain::default(),
        BTreeMap::from([("vendor.example".to_owned(), json!({"preserved": true}))]),
    ))
}

fn set_marker(
    mut document: ScenarioDocument,
    marker: Option<i64>,
    updated_at: &str,
) -> Result<ScenarioDocument, StoreError> {
    match marker {
        Some(value) => {
            document
                .extensions
                .insert("test.marker".to_owned(), json!(value));
        }
        None => {
            document.extensions.remove("test.marker");
        }
    }
    document.metadata.updated_at = Rfc3339Timestamp::parse(updated_at)
        .map_err(|error| StoreError::Integrity(error.to_string()))?;
    Ok(document)
}

fn journal(
    command: Value,
    inverse: Option<Value>,
    created_at: &str,
) -> Result<JournalWrite, StoreError> {
    Ok(JournalWrite {
        command_type: "set_marker".to_owned(),
        command,
        command_id: CommandId::from_uuid(Uuid::now_v7()),
        inverse,
        actor: ActorRef {
            actor_id: Some("store-test".to_owned()),
            display_name: "Store test".to_owned(),
        },
        source: CommandSource::System,
        summary: "Set marker".to_owned(),
        created_at: Rfc3339Timestamp::parse(created_at)
            .map_err(|error| StoreError::Integrity(error.to_string()))?,
    })
}

fn staged_import(
    local_library_revision: Revision,
    scenarios: Vec<(Revision, ScenarioDocument, StagedDisposition)>,
    source_created_at: Rfc3339Timestamp,
) -> StagedImport {
    StagedImport {
        binding: PreviewBinding {
            file_sha256: "staged-test".to_owned(),
            options_sha256: "staged-test".to_owned(),
            local_library_revision,
            format_version: 1,
            schema_version: 1,
        },
        mode: RestoreMode::ImportScenario,
        scenarios: scenarios
            .into_iter()
            .map(|(revision, document, disposition)| StagedScenario {
                original_id: document.scenario_id,
                source_revision: revision,
                disposition,
                scenario: PortableScenario::current(revision, document, BTreeSet::new()),
                id_remap: BTreeMap::new(),
            })
            .collect(),
        scenario_revisions: Vec::new(),
        results: BTreeMap::new(),
        shared_records: BTreeMap::new(),
        preferences: BTreeMap::new(),
        manifest_extensions: BTreeMap::new(),
        nonsemantic_extensions: BTreeSet::new(),
        assets: BTreeMap::new(),
        supplemental_replacements: BTreeSet::new(),
        provenance: ImportProvenance {
            source_bundle_id: BundleId::from_uuid(Uuid::now_v7()),
            source_application: ApplicationMetadata {
                name: "store-test".to_owned(),
                version: "1".to_owned(),
            },
            original_format_version: 1,
            original_schema_version: 1,
            source_file_sha256: "staged-test".to_owned(),
            source_created_at,
            applied_migrations: Vec::new(),
        },
    }
}

#[tokio::test]
async fn project_crud_and_document_survive_reopen() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(1)?;
    let expected = document(id)?;

    let (store, first_start) = SqliteScenarioStore::open(&path).await?;
    assert_eq!(first_start.applied_migrations, vec![1]);
    let created = store
        .create_project(NewProject {
            document: expected.clone(),
        })
        .await?;
    assert_eq!(created.summary.revision, Revision::INITIAL);
    assert_eq!(created.document, expected);
    store
        .archive_project(id, Revision::INITIAL, timestamp(UPDATED)?)
        .await?;
    assert_eq!(
        store.list_projects(ProjectListScope::Active).await?.len(),
        0
    );
    assert_eq!(
        store.list_projects(ProjectListScope::Archived).await?.len(),
        1
    );
    store.unarchive_project(id, Revision::INITIAL).await?;
    drop(store);

    let (reopened, second_start) = SqliteScenarioStore::open(&path).await?;
    assert!(second_start.applied_migrations.is_empty());
    let persisted = reopened.get_project(id).await?;
    assert_eq!(persisted.summary.revision, Revision::INITIAL);
    assert_eq!(persisted.document, expected);
    assert_eq!(
        reopened
            .list_projects(ProjectListScope::Active)
            .await?
            .len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn direct_create_rejects_duplicate_identity_occurrences_without_artifacts()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let root_id = scenario_id(110)?;
    let mut root_collision = document(root_id)?;
    let root_person = PersonId::from_uuid(root_id.as_uuid());
    root_collision
        .domain
        .entities
        .insert(root_person, json!({"id": root_person}));
    let entity_rule_id = Uuid::now_v7();
    let mut typed_collision = document(scenario_id(111)?)?;
    let person = PersonId::from_uuid(entity_rule_id);
    let rule = RuleId::from_uuid(entity_rule_id);
    typed_collision
        .domain
        .entities
        .insert(person, json!({"id": person}));
    typed_collision
        .domain
        .rules
        .insert(rule, json!({"id": rule}));

    let (store, _) = SqliteScenarioStore::open(&path).await?;
    for invalid in [root_collision, typed_collision] {
        assert!(matches!(
            store.create_project(NewProject { document: invalid }).await,
            Err(StoreError::InvalidScenarioIdentity(_))
        ));
        let snapshot = store.library_snapshot().await?;
        assert_eq!(snapshot.revision, Revision::INITIAL);
        assert!(snapshot.projects.is_empty());
        assert!(snapshot.scenario_revision_high_water.is_empty());
    }
    drop(store);

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    let snapshot = reopened.library_snapshot().await?;
    assert!(snapshot.projects.is_empty());
    assert!(snapshot.scenario_revision_high_water.is_empty());
    Ok(())
}

#[tokio::test]
async fn staged_apply_rejects_global_identity_collision_atomically() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let (store, _) = SqliteScenarioStore::open(&directory.path().join("library.sqlite3")).await?;
    let id = scenario_id(91)?;
    store
        .create_project(NewProject {
            document: document(id)?,
        })
        .await?;
    let before = store.library_snapshot().await?;
    let mut staged = staged_import(before.revision, Vec::new(), timestamp(CREATED)?);
    let key = format!("{id}.json");
    staged
        .shared_records
        .insert(key.clone(), br#"{"value":"synthetic"}"#.to_vec());

    assert!(matches!(
        store
            .apply_staged_library(StagedLibraryApply::Import(staged), timestamp(LATER)?)
            .await,
        Err(StoreError::IdentityCollision(_))
    ));
    let after = store.library_snapshot().await?;
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.projects.len(), before.projects.len());
    assert_eq!(after.projects[0].summary.id, before.projects[0].summary.id);
    assert_eq!(
        after.projects[0].summary.revision,
        before.projects[0].summary.revision
    );
    assert_eq!(after.projects[0].document, before.projects[0].document);
    assert!(!after.sections.shared_records.contains_key(&key));
    Ok(())
}

#[tokio::test]
async fn command_rejects_identity_owned_by_another_project_atomically() -> Result<(), Box<dyn Error>>
{
    let directory = tempdir()?;
    let (store, _) = SqliteScenarioStore::open(&directory.path().join("library.sqlite3")).await?;
    let owner_id = scenario_id(92)?;
    let target_id = scenario_id(93)?;
    let shared_uuid = scenario_id(94)?.as_uuid();
    let person_id: PersonId = shared_uuid.to_string().parse()?;
    let rule_id: RuleId = shared_uuid.to_string().parse()?;
    let mut owner = document(owner_id)?;
    owner.domain.entities.insert(
        person_id,
        json!({"id": shared_uuid, "name": "existing owner"}),
    );
    store.create_project(NewProject { document: owner }).await?;
    store
        .create_project(NewProject {
            document: document(target_id)?,
        })
        .await?;
    let before = store.library_snapshot().await?;

    assert!(matches!(
        store
            .execute_command(
                target_id,
                Revision::INITIAL,
                RedoBranchPolicy::Reject,
                move |current| {
                    let mut updated = current.clone();
                    updated
                        .domain
                        .rules
                        .insert(rule_id, json!({"id": shared_uuid, "required": true}));
                    Ok(CommandWrite {
                        document: updated,
                        journal: journal(json!({"addRule": shared_uuid}), None, UPDATED)?,
                        output: (),
                    })
                },
            )
            .await,
        Err(StoreError::IdentityCollision(id)) if id == shared_uuid
    ));
    let after = store.library_snapshot().await?;
    assert_eq!(after.revision, before.revision);
    assert_eq!(
        store.get_project(target_id).await?.summary.revision,
        Revision::INITIAL
    );
    assert!(
        store
            .get_project(target_id)
            .await?
            .document
            .domain
            .rules
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn removed_identity_remains_reserved_for_undo() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let (store, _) = SqliteScenarioStore::open(&directory.path().join("library.sqlite3")).await?;
    let owner_id = scenario_id(95)?;
    let target_id = scenario_id(96)?;
    let reserved_uuid = scenario_id(97)?.as_uuid();
    let person_id: PersonId = reserved_uuid.to_string().parse()?;
    let rule_id: RuleId = reserved_uuid.to_string().parse()?;
    store
        .create_project(NewProject {
            document: document(owner_id)?,
        })
        .await?;
    store
        .execute_command(
            owner_id,
            Revision::INITIAL,
            RedoBranchPolicy::Reject,
            move |current| {
                let mut updated = current.clone();
                updated.domain.entities.insert(
                    person_id,
                    json!({"id": reserved_uuid, "name": "temporarily present"}),
                );
                Ok(CommandWrite {
                    document: updated,
                    journal: journal(
                        json!({"addPerson": reserved_uuid}),
                        Some(json!({"removePerson": reserved_uuid})),
                        UPDATED,
                    )?,
                    output: (),
                })
            },
        )
        .await?;
    store
        .execute_command(
            owner_id,
            Revision::new(1),
            RedoBranchPolicy::Reject,
            move |current| {
                let mut updated = current.clone();
                updated.domain.entities.remove(&person_id);
                Ok(CommandWrite {
                    document: updated,
                    journal: journal(
                        json!({"removePerson": reserved_uuid}),
                        Some(json!({"addPerson": reserved_uuid})),
                        LATER,
                    )?,
                    output: (),
                })
            },
        )
        .await?;
    store
        .create_project(NewProject {
            document: document(target_id)?,
        })
        .await?;

    assert_eq!(
        store
            .library_snapshot()
            .await?
            .scenario_identity_owners
            .get(&reserved_uuid),
        Some(&owner_id)
    );
    assert!(matches!(
        store
            .execute_command(
                target_id,
                Revision::INITIAL,
                RedoBranchPolicy::Reject,
                move |current| {
                    let mut updated = current.clone();
                    updated.domain.rules.insert(
                        rule_id,
                        json!({"id": reserved_uuid, "required": true}),
                    );
                    Ok(CommandWrite {
                        document: updated,
                        journal: journal(json!({"addRule": reserved_uuid}), None, LATER)?,
                        output: (),
                    })
                },
            )
            .await,
        Err(StoreError::IdentityCollision(id)) if id == reserved_uuid
    ));

    store
        .undo(
            owner_id,
            Revision::new(2),
            timestamp(LATER)?,
            move |history| {
                let mut restored = history.document;
                restored.domain.entities.insert(
                    person_id,
                    json!({"id": reserved_uuid, "name": "restored by undo"}),
                );
                Ok((restored, ()))
            },
        )
        .await?;
    assert!(
        store
            .get_project(owner_id)
            .await?
            .document
            .domain
            .entities
            .contains_key(&person_id)
    );
    Ok(())
}

#[tokio::test]
async fn publication_revision_lease_blocks_other_store_instances() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let (first, _) = SqliteScenarioStore::open(&path).await?;
    let (second, _) = SqliteScenarioStore::open(&path).await?;
    let first = Arc::new(first);
    let second = Arc::new(second);
    let expected = first.library_metadata_snapshot().await?.revision;
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let lease_store = Arc::clone(&first);
    let lease = tokio::spawn(async move {
        lease_store
            .with_publication_revision_lease(expected, None, move || {
                let _ = entered_tx.send(());
                release_rx.recv()?;
                Ok::<(), std::sync::mpsc::RecvError>(())
            })
            .await
    });
    entered_rx.await?;

    let (attempt_tx, attempt_rx) = tokio::sync::oneshot::channel();
    let updated = timestamp(UPDATED)?;
    let mutation_store = Arc::clone(&second);
    let mutation = tokio::spawn(async move {
        let _ = attempt_tx.send(());
        mutation_store
            .set_setting("appearance".to_owned(), json!({"theme": "dark"}), updated)
            .await
    });
    attempt_rx.await?;
    tokio::task::yield_now().await;
    assert!(!mutation.is_finished());
    release_tx.send(())?;
    lease.await???;
    mutation.await??;

    let next_revision = expected.checked_next()?;
    let callback_called = Arc::new(AtomicBool::new(false));
    let callback_flag = Arc::clone(&callback_called);
    assert!(matches!(
        first
            .with_publication_revision_lease(expected, None, move || {
                callback_flag.store(true, Ordering::SeqCst);
                Ok::<(), std::convert::Infallible>(())
            })
            .await,
        Err(StoreError::LibraryConflict {
            expected: stale,
            actual
        }) if stale == expected && actual == next_revision
    ));
    assert!(!callback_called.load(Ordering::SeqCst));
    Ok(())
}

async fn apply_and_assert_revision_bound_import(
    store: &SqliteScenarioStore,
    existing_id: ScenarioId,
    created_id: ScenarioId,
    copied_id: ScenarioId,
) -> Result<(), Box<dyn Error>> {
    assert_eq!(store.library_snapshot().await?.revision, Revision::INITIAL);
    store
        .create_project(NewProject {
            document: document(existing_id)?,
        })
        .await?;

    let mut replacement = document(existing_id)?;
    "Replaced plan".clone_into(&mut replacement.metadata.title);
    let staged = staged_import(
        Revision::new(1),
        vec![
            (Revision::new(7), replacement, StagedDisposition::Replace),
            (
                Revision::new(8),
                document(created_id)?,
                StagedDisposition::Create,
            ),
            (
                Revision::new(9),
                document(copied_id)?,
                StagedDisposition::CreateCopy,
            ),
        ],
        timestamp(CREATED)?,
    );
    let outcome = store
        .apply_staged_library(
            StagedLibraryApply::Import(staged.clone()),
            timestamp(LATER)?,
        )
        .await?;
    assert_eq!(outcome.library_revision, Revision::new(2));
    assert_eq!(outcome.created, 2);
    assert_eq!(outcome.replaced, 1);
    let snapshot = store.library_snapshot().await?;
    assert_eq!(snapshot.revision, Revision::new(2));
    assert_eq!(snapshot.projects.len(), 3);
    assert_eq!(
        store
            .get_project(existing_id)
            .await?
            .document
            .metadata
            .title,
        "Replaced plan"
    );
    assert_eq!(
        store.get_project(existing_id).await?.summary.revision,
        Revision::new(7)
    );
    assert_eq!(
        store.get_project(created_id).await?.summary.revision,
        Revision::new(8)
    );
    assert_eq!(
        store.get_project(copied_id).await?.summary.revision,
        Revision::new(9)
    );
    assert!(matches!(
        store
            .apply_staged_library(StagedLibraryApply::Import(staged), timestamp(LATER)?)
            .await,
        Err(StoreError::LibraryConflict { expected, actual })
            if expected == Revision::new(1) && actual == Revision::new(2)
    ));
    Ok(())
}

async fn assert_failed_staged_imports_are_atomic(
    store: &SqliteScenarioStore,
    existing_id: ScenarioId,
    created_id: ScenarioId,
    rolled_back_id: ScenarioId,
) -> Result<(), Box<dyn Error>> {
    let impossible = staged_import(
        Revision::new(2),
        vec![
            (
                Revision::new(10),
                document(rolled_back_id)?,
                StagedDisposition::Create,
            ),
            (
                Revision::new(7),
                document(existing_id)?,
                StagedDisposition::Replace,
            ),
        ],
        timestamp(CREATED)?,
    );
    assert!(matches!(
        store
            .apply_staged_library(StagedLibraryApply::Import(impossible), timestamp(LATER)?)
            .await,
        Err(StoreError::InvalidStagedApply(_))
    ));
    assert!(matches!(
        store.get_project(rolled_back_id).await,
        Err(StoreError::ScenarioNotFound(id)) if id == rolled_back_id
    ));
    let preserved = store.get_project(existing_id).await?;
    assert_eq!(preserved.summary.revision, Revision::new(7));
    assert_eq!(preserved.document.metadata.title, "Replaced plan");
    assert_eq!(store.library_snapshot().await?.revision, Revision::new(2));

    let invalid = staged_import(
        Revision::new(2),
        vec![
            (
                Revision::new(10),
                document(rolled_back_id)?,
                StagedDisposition::Create,
            ),
            (
                Revision::new(11),
                document(created_id)?,
                StagedDisposition::Create,
            ),
        ],
        timestamp(CREATED)?,
    );
    assert!(matches!(
        store
            .apply_staged_library(StagedLibraryApply::Import(invalid), timestamp(LATER)?)
            .await,
        Err(StoreError::ScenarioAlreadyExists(id)) if id == created_id
    ));
    assert!(matches!(
        store.get_project(rolled_back_id).await,
        Err(StoreError::ScenarioNotFound(id)) if id == rolled_back_id
    ));
    assert_eq!(store.library_snapshot().await?.revision, Revision::new(2));
    Ok(())
}

async fn apply_and_assert_backup_restore(
    store: &SqliteScenarioStore,
    existing_id: ScenarioId,
    created_id: ScenarioId,
    copied_id: ScenarioId,
    restored_id: ScenarioId,
) -> Result<(), Box<dyn Error>> {
    let mut replacement = staged_import(
        Revision::new(2),
        vec![(
            Revision::new(12),
            document(restored_id)?,
            StagedDisposition::Create,
        )],
        timestamp(CREATED)?,
    );
    replacement.mode = RestoreMode::ReplaceLibrary;
    let restore = StagedBackupRestore {
        import: replacement,
        remove_scenario_ids: BTreeSet::from([existing_id, created_id, copied_id]),
        authorization: RestoreAuthorization {
            destructive_action_confirmed: true,
            prospective_failure_receipt_token: None,
            collision_plan_sha256: Some("a".repeat(64)),
            safety_backup: SafetyBackupEvidence::Verified {
                bundle_sha256: "verified-test-backup".to_owned(),
            },
        },
    };
    let outcome = store
        .apply_staged_library(
            StagedLibraryApply::BackupRestore {
                restore,
                settings: BTreeMap::new(),
            },
            timestamp(LATER)?,
        )
        .await?;
    assert_eq!(outcome.library_revision, Revision::new(3));
    assert_eq!(outcome.removed, 3);
    assert_eq!(store.library_snapshot().await?.projects.len(), 1);
    let restored = store.get_project(restored_id).await?;
    assert_eq!(restored.document.scenario_id, restored_id);
    assert_eq!(restored.summary.revision, Revision::new(12));
    Ok(())
}

#[tokio::test]
async fn staged_library_apply_is_atomic_and_revision_bound() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let existing_id = scenario_id(20)?;
    let created_id = scenario_id(21)?;
    let rolled_back_id = scenario_id(22)?;
    let copied_id = scenario_id(23)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;

    apply_and_assert_revision_bound_import(&store, existing_id, created_id, copied_id).await?;
    assert_failed_staged_imports_are_atomic(&store, existing_id, created_id, rolled_back_id)
        .await?;
    apply_and_assert_backup_restore(&store, existing_id, created_id, copied_id, rolled_back_id)
        .await?;
    Ok(())
}

async fn seed_portable_import(
    store: &SqliteScenarioStore,
    local_id: ScenarioId,
) -> Result<BundleId, Box<dyn Error>> {
    store
        .create_project(NewProject {
            document: document(local_id)?,
        })
        .await?;
    store
        .set_setting(
            "appearance".to_owned(),
            json!({"theme": "dark"}),
            timestamp(UPDATED)?,
        )
        .await?;
    let mut imported = staged_import(
        Revision::new(2),
        vec![(
            Revision::new(7),
            document(local_id)?,
            StagedDisposition::Replace,
        )],
        timestamp(CREATED)?,
    );
    imported.results.insert(
        "result-a".to_owned(),
        serde_json::to_vec(&json!({
            "resultId": "018f1e2d-3c4b-7a69-8def-200000000001",
            "scenarioId": local_id,
            "scenarioRevision": 7,
            "value": 1
        }))?,
    );
    imported
        .manifest_extensions
        .insert("vendor.manifest".to_owned(), json!({"preserved": true}));
    imported
        .nonsemantic_extensions
        .insert("vendor.manifest".to_owned());
    imported
        .shared_records
        .insert("shared-a".to_owned(), br#"{"value":2}"#.to_vec());
    imported
        .preferences
        .insert("preference-a".to_owned(), br#"{"value":3}"#.to_vec());
    imported.assets.insert(
        "asset-a".to_owned(),
        PortableAsset {
            bytes: b"asset bytes".to_vec(),
            media_type: "text/plain".to_owned(),
            redistribution_permitted: false,
        },
    );
    imported.scenarios[0]
        .scenario
        .required_capabilities
        .insert(SemanticCapability {
            id: "vendor.semantic".to_owned(),
            version: 2,
        });
    imported.scenarios[0]
        .scenario
        .semantic_extensions
        .insert("vendor.semantic".to_owned(), json!({"meaning": 1}));
    imported.scenarios[0]
        .scenario
        .extensions
        .insert("vendor.display".to_owned(), json!({"color": "blue"}));
    let source_bundle_id = imported.provenance.source_bundle_id;
    store
        .apply_staged_library(StagedLibraryApply::Import(imported), timestamp(LATER)?)
        .await?;
    Ok(source_bundle_id)
}

async fn apply_add_backup(
    store: &SqliteScenarioStore,
    archived_id: ScenarioId,
) -> Result<(), Box<dyn Error>> {
    let mut added = staged_import(
        Revision::new(3),
        vec![(
            Revision::new(12),
            document(archived_id)?,
            StagedDisposition::Create,
        )],
        timestamp(CREATED)?,
    );
    added.mode = RestoreMode::AddBackup;
    added.scenarios[0].scenario.project = Some(PortableProjectMetadata {
        archived_at: Some(timestamp(UPDATED)?),
    });
    added.results.insert(
        "result-b".to_owned(),
        serde_json::to_vec(&json!({
            "resultId": Uuid::now_v7(),
            "scenarioId": archived_id,
            "scenarioRevision": 12,
            "value": 4
        }))?,
    );
    store
        .apply_staged_library(
            StagedLibraryApply::BackupRestore {
                restore: StagedBackupRestore {
                    import: added,
                    remove_scenario_ids: BTreeSet::new(),
                    authorization: RestoreAuthorization {
                        destructive_action_confirmed: false,
                        prospective_failure_receipt_token: None,
                        collision_plan_sha256: None,
                        safety_backup: SafetyBackupEvidence::NotRequired,
                    },
                },
                settings: BTreeMap::from([(
                    "backup-added".to_owned(),
                    AppSetting {
                        value: json!({"enabled": true}),
                        updated_at: timestamp(UPDATED)?,
                    },
                )]),
            },
            timestamp(LATER)?,
        )
        .await?;
    Ok(())
}

fn failed_receipt_restore(
    local_revision: Revision,
    scenario_id: ScenarioId,
    proof: &str,
    collision_plan_sha256: &str,
) -> Result<StagedLibraryApply, Box<dyn Error>> {
    let mut import = staged_import(
        local_revision,
        vec![(
            Revision::new(5),
            document(scenario_id)?,
            StagedDisposition::Replace,
        )],
        timestamp(CREATED)?,
    );
    import.mode = RestoreMode::ReplaceLibrary;
    Ok(StagedLibraryApply::BackupRestore {
        restore: StagedBackupRestore {
            import,
            remove_scenario_ids: BTreeSet::from([scenario_id]),
            authorization: RestoreAuthorization {
                destructive_action_confirmed: true,
                prospective_failure_receipt_token: None,
                collision_plan_sha256: Some(collision_plan_sha256.to_owned()),
                safety_backup: SafetyBackupEvidence::FailedWithStrongConfirmation {
                    proof: proof.to_owned(),
                },
            },
        },
        settings: BTreeMap::new(),
    })
}

fn staged_apply_binding(staged: &StagedLibraryApply) -> PreviewBinding {
    match staged {
        StagedLibraryApply::Import(import) => import.binding.clone(),
        StagedLibraryApply::BackupRestore { restore, .. } => restore.import.binding.clone(),
    }
}

fn receipt_count(path: &std::path::Path) -> Result<i64, rusqlite::Error> {
    Connection::open(path)?.query_row(
        "SELECT COUNT(*) FROM safety_backup_failure_receipts",
        [],
        |row| row.get(0),
    )
}

async fn record_failure_receipt(
    store: &SqliteScenarioStore,
    proof: &str,
    binding: PreviewBinding,
    collision_plan_sha256: &str,
) -> Result<(), Box<dyn Error>> {
    store
        .record_safety_backup_failure_receipt(SafetyBackupFailureReceipt {
            proof: proof.to_owned(),
            binding,
            collision_plan_sha256: collision_plan_sha256.to_owned(),
            safe_reason: "destination refused the backup".to_owned(),
            created_at: timestamp(UPDATED)?,
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn failure_receipt_persists_restarts_consumes_once_and_is_bounded()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(92)?;
    let proof = "owner-private-first-use-proof";
    let plan = "a".repeat(64);
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(id)?,
        })
        .await?;
    let staged = failed_receipt_restore(Revision::new(1), id, proof, &plan)?;
    record_failure_receipt(&store, proof, staged_apply_binding(&staged), &plan).await?;
    drop(store);

    let connection = Connection::open(&path)?;
    let (stored_hash, stored_binding, stored_plan, stored_reason): (
        String,
        Vec<u8>,
        String,
        String,
    ) = connection.query_row(
        "SELECT proof_sha256, binding_json, collision_plan_sha256, safe_reason FROM safety_backup_failure_receipts",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    assert_eq!(stored_hash, eutheto_export::sha256_hex(proof.as_bytes()));
    assert!(!String::from_utf8(stored_binding)?.contains(proof));
    assert!(!stored_plan.contains(proof));
    assert!(!stored_reason.contains(proof));
    drop(connection);
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .apply_staged_library(staged, timestamp(LATER)?)
        .await?;
    assert_eq!(receipt_count(&path)?, 0);

    let replay = failed_receipt_restore(Revision::new(2), id, proof, &plan)?;
    assert!(matches!(
        store.apply_staged_library(replay, timestamp(LATER)?).await,
        Err(StoreError::SafetyBackupFailureReceiptRejected)
    ));
    for index in 0..17 {
        let proof = format!("bounded-proof-{index}");
        let binding = staged_import(Revision::new(2), Vec::new(), timestamp(CREATED)?).binding;
        record_failure_receipt(&store, &proof, binding, &plan).await?;
    }
    assert_eq!(receipt_count(&path)?, 16);
    Ok(())
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn failure_receipt_mismatch_and_failed_apply_do_not_consume() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(93)?;
    let proof = "rollback-bound-proof";
    let plan = "b".repeat(64);
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(id)?,
        })
        .await?;
    let staged = failed_receipt_restore(Revision::new(1), id, proof, &plan)?;
    record_failure_receipt(&store, proof, staged_apply_binding(&staged), &plan).await?;

    let plan_mismatch = failed_receipt_restore(Revision::new(1), id, proof, &"c".repeat(64))?;
    assert!(matches!(
        store
            .apply_staged_library(plan_mismatch, timestamp(LATER)?)
            .await,
        Err(StoreError::SafetyBackupFailureReceiptRejected)
    ));
    let mut binding_mismatch = staged.clone();
    if let StagedLibraryApply::BackupRestore { restore, .. } = &mut binding_mismatch {
        restore.import.binding.options_sha256 = "different-binding".to_owned();
    }
    assert!(matches!(
        store
            .apply_staged_library(binding_mismatch, timestamp(LATER)?)
            .await,
        Err(StoreError::SafetyBackupFailureReceiptRejected)
    ));
    assert_eq!(receipt_count(&path)?, 1);
    assert_eq!(
        store.get_project(id).await?.summary.revision,
        Revision::INITIAL
    );

    store.set_failpoint(Failpoint::AfterSupplementalWrite)?;
    assert!(matches!(
        store
            .apply_staged_library(staged.clone(), timestamp(LATER)?)
            .await,
        Err(StoreError::InjectedFailure)
    ));
    assert_eq!(receipt_count(&path)?, 1);
    assert_eq!(
        store.get_project(id).await?.summary.revision,
        Revision::INITIAL
    );
    store
        .apply_staged_library(staged, timestamp(LATER)?)
        .await?;
    assert_eq!(receipt_count(&path)?, 0);
    Ok(())
}

#[tokio::test]
async fn portable_sections_wrapper_metadata_and_provenance_survive_reopen()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let local_id = scenario_id(30)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    let source_bundle_id = seed_portable_import(&store, local_id).await?;
    drop(store);

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    let imported_project = reopened.get_project(local_id).await?;
    assert_eq!(
        imported_project
            .portable
            .required_capabilities
            .iter()
            .next()
            .map(|capability| (capability.id.as_str(), capability.version)),
        Some(("vendor.semantic", 2))
    );
    assert_eq!(imported_project.summary.revision, Revision::new(7));
    assert_eq!(
        imported_project.portable.semantic_extensions["vendor.semantic"],
        json!({"meaning": 1})
    );
    assert_eq!(
        imported_project.portable.extensions["vendor.display"],
        json!({"color": "blue"})
    );
    let snapshot = reopened.library_snapshot().await?;
    assert_eq!(
        snapshot.settings["appearance"].value,
        json!({"theme": "dark"})
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&snapshot.sections.results["result-a"])?,
        json!({
            "resultId": "018f1e2d-3c4b-7a69-8def-200000000001",
            "scenarioId": local_id,
            "scenarioRevision": 7,
            "value": 1
        })
    );
    assert_eq!(
        snapshot.manifest_extensions["vendor.manifest"],
        json!({"preserved": true})
    );
    assert!(snapshot.nonsemantic_extensions.contains("vendor.manifest"));
    assert_eq!(
        snapshot.sections.shared_records["shared-a"],
        br#"{"value":2}"#
    );
    assert_eq!(
        snapshot.sections.preferences["preference-a"],
        br#"{"value":3}"#
    );
    assert_eq!(snapshot.sections.assets["asset-a"].bytes, b"asset bytes");
    assert_eq!(snapshot.sections.assets["asset-a"].media_type, "text/plain");
    assert!(!snapshot.sections.assets["asset-a"].redistribution_permitted);
    assert_eq!(
        snapshot.supplemental_identities,
        BTreeSet::from([
            SupplementalIdentity {
                section: SupplementalSectionKind::Results,
                key: "result-a".to_owned(),
            },
            SupplementalIdentity {
                section: SupplementalSectionKind::SharedRecords,
                key: "shared-a".to_owned(),
            },
            SupplementalIdentity {
                section: SupplementalSectionKind::Preferences,
                key: "preference-a".to_owned(),
            },
            SupplementalIdentity {
                section: SupplementalSectionKind::Assets,
                key: "asset-a".to_owned(),
            },
        ])
    );
    assert_eq!(snapshot.provenance.len(), 1);
    assert_eq!(snapshot.provenance[0].source_bundle_id, source_bundle_id);
    assert_eq!(
        snapshot.provenance[0].scenario_sources[0].source_revision,
        Revision::new(7)
    );
    Ok(())
}

#[tokio::test]
async fn supplemental_overwrite_requires_exact_replace_authorization_and_skip_is_absent()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    let replace_identity = SupplementalIdentity {
        section: SupplementalSectionKind::SharedRecords,
        key: "replace-me".to_owned(),
    };
    let skip_identity = SupplementalIdentity {
        section: SupplementalSectionKind::SharedRecords,
        key: "keep-me".to_owned(),
    };
    let mut baseline = staged_import(Revision::INITIAL, Vec::new(), timestamp(CREATED)?);
    baseline.shared_records.insert(
        replace_identity.key.clone(),
        br#"{"value":"before"}"#.to_vec(),
    );
    baseline
        .shared_records
        .insert(skip_identity.key.clone(), br#"{"value":"keep"}"#.to_vec());
    store
        .apply_staged_library(StagedLibraryApply::Import(baseline), timestamp(LATER)?)
        .await?;

    let mut unauthorized = staged_import(Revision::new(1), Vec::new(), timestamp(CREATED)?);
    unauthorized.shared_records.insert(
        replace_identity.key.clone(),
        br#"{"value":"unauthorized"}"#.to_vec(),
    );
    assert!(matches!(
        store
            .apply_staged_library(StagedLibraryApply::Import(unauthorized), timestamp(LATER)?)
            .await,
        Err(StoreError::InvalidStagedApply(_))
    ));
    let after_unauthorized = store.library_snapshot().await?;
    assert_eq!(after_unauthorized.revision, Revision::new(1));
    assert_eq!(
        after_unauthorized.sections.shared_records[&replace_identity.key],
        br#"{"value":"before"}"#
    );

    let mut absent_skip = staged_import(Revision::new(1), Vec::new(), timestamp(CREATED)?);
    absent_skip
        .supplemental_replacements
        .insert(skip_identity.clone());
    assert!(matches!(
        store
            .apply_staged_library(StagedLibraryApply::Import(absent_skip), timestamp(LATER)?)
            .await,
        Err(StoreError::InvalidStagedApply(_))
    ));

    let mut authorized = staged_import(Revision::new(1), Vec::new(), timestamp(CREATED)?);
    authorized.shared_records.insert(
        replace_identity.key.clone(),
        br#"{"value":"after"}"#.to_vec(),
    );
    authorized
        .supplemental_replacements
        .insert(replace_identity.clone());
    store
        .apply_staged_library(StagedLibraryApply::Import(authorized), timestamp(LATER)?)
        .await?;
    let snapshot = store.library_snapshot().await?;
    assert_eq!(
        snapshot.sections.shared_records[&replace_identity.key],
        br#"{"value":"after"}"#
    );
    assert_eq!(
        snapshot.sections.shared_records[&skip_identity.key],
        br#"{"value":"keep"}"#
    );
    Ok(())
}

#[tokio::test]
async fn add_backup_merges_settings_sections_and_archived_state() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let local_id = scenario_id(30)?;
    let archived_id = scenario_id(31)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    seed_portable_import(&store, local_id).await?;
    apply_add_backup(&store, archived_id).await?;

    let archived = store.get_project(archived_id).await?;
    assert_eq!(archived.summary.revision, Revision::new(12));
    assert_eq!(archived.summary.archived_at, Some(timestamp(UPDATED)?));
    let snapshot = store.library_snapshot().await?;
    assert!(snapshot.sections.results.contains_key("result-a"));
    assert!(snapshot.sections.results.contains_key("result-b"));
    assert_eq!(snapshot.provenance.len(), 2);
    assert_eq!(
        snapshot.settings["backup-added"].value,
        json!({"enabled": true})
    );
    assert_eq!(
        snapshot.settings["appearance"].value,
        json!({"theme": "dark"})
    );
    Ok(())
}

#[tokio::test]
async fn replace_backup_replaces_settings_sections_and_preserves_archive_on_reopen()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let local_id = scenario_id(30)?;
    let archived_id = scenario_id(31)?;
    let restored_id = scenario_id(32)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    seed_portable_import(&store, local_id).await?;
    apply_add_backup(&store, archived_id).await?;
    let mut replacement = staged_import(
        Revision::new(4),
        vec![(
            Revision::new(13),
            document(restored_id)?,
            StagedDisposition::Create,
        )],
        timestamp(CREATED)?,
    );
    replacement.mode = RestoreMode::ReplaceLibrary;
    replacement.scenarios[0].scenario.project = Some(PortableProjectMetadata {
        archived_at: Some(timestamp(LATER)?),
    });
    replacement
        .preferences
        .insert("replacement".to_owned(), br#"{"only":true}"#.to_vec());
    store
        .apply_staged_library(
            StagedLibraryApply::BackupRestore {
                restore: StagedBackupRestore {
                    import: replacement,
                    remove_scenario_ids: BTreeSet::from([local_id, archived_id]),
                    authorization: RestoreAuthorization {
                        destructive_action_confirmed: true,
                        prospective_failure_receipt_token: None,
                        collision_plan_sha256: Some("a".repeat(64)),
                        safety_backup: SafetyBackupEvidence::Verified {
                            bundle_sha256: "verified-replacement".to_owned(),
                        },
                    },
                },
                settings: BTreeMap::from([(
                    "appearance".to_owned(),
                    AppSetting {
                        value: json!({"theme": "restored"}),
                        updated_at: timestamp(LATER)?,
                    },
                )]),
            },
            timestamp(LATER)?,
        )
        .await?;
    drop(store);

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    let restored = reopened.get_project(restored_id).await?;
    assert_eq!(restored.summary.revision, Revision::new(13));
    assert_eq!(restored.summary.archived_at, Some(timestamp(LATER)?));
    let snapshot = reopened.library_snapshot().await?;
    assert!(snapshot.sections.results.is_empty());
    assert!(snapshot.sections.shared_records.is_empty());
    assert!(snapshot.sections.assets.is_empty());
    assert!(snapshot.manifest_extensions.is_empty());
    assert!(snapshot.nonsemantic_extensions.is_empty());
    assert_eq!(
        snapshot.sections.preferences["replacement"],
        br#"{"only":true}"#
    );
    assert_eq!(snapshot.provenance.len(), 1);
    assert_eq!(
        snapshot.settings["appearance"].value,
        json!({"theme": "restored"})
    );
    assert!(!snapshot.settings.contains_key("backup-added"));
    Ok(())
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn supplemental_failpoint_rolls_back_scenarios_sections_and_provenance()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let existing_id = scenario_id(33)?;
    let imported_id = scenario_id(34)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(existing_id)?,
        })
        .await?;
    let mut baseline = staged_import(Revision::new(1), Vec::new(), timestamp(CREATED)?);
    baseline
        .shared_records
        .insert("stable".to_owned(), br#"{"value":"before"}"#.to_vec());
    store
        .apply_staged_library(StagedLibraryApply::Import(baseline), timestamp(LATER)?)
        .await?;
    store
        .set_setting(
            "stable-setting".to_owned(),
            json!({"value": "before"}),
            timestamp(UPDATED)?,
        )
        .await?;
    store.set_failpoint(Failpoint::AfterSupplementalWrite)?;

    let mut staged = staged_import(
        Revision::new(3),
        vec![(
            Revision::new(4),
            document(imported_id)?,
            StagedDisposition::Create,
        )],
        timestamp(CREATED)?,
    );
    staged
        .shared_records
        .insert("stable".to_owned(), br#"{"value":"after"}"#.to_vec());
    staged.assets.insert(
        "rolled-back".to_owned(),
        PortableAsset {
            bytes: b"rolled back".to_vec(),
            media_type: "text/plain".to_owned(),
            redistribution_permitted: true,
        },
    );
    staged.mode = RestoreMode::ReplaceLibrary;
    assert!(matches!(
        store
            .apply_staged_library(
                StagedLibraryApply::BackupRestore {
                    restore: StagedBackupRestore {
                        import: staged,
                        remove_scenario_ids: BTreeSet::from([existing_id]),
                        authorization: RestoreAuthorization {
                            destructive_action_confirmed: true,
                            prospective_failure_receipt_token: None,
                            collision_plan_sha256: Some("a".repeat(64)),
                            safety_backup: SafetyBackupEvidence::Verified {
                                bundle_sha256: "verified-failpoint".to_owned(),
                            },
                        },
                    },
                    settings: BTreeMap::from([(
                        "rolled-back-setting".to_owned(),
                        AppSetting {
                            value: json!({"value": "after"}),
                            updated_at: timestamp(LATER)?,
                        },
                    )]),
                },
                timestamp(LATER)?
            )
            .await,
        Err(StoreError::InjectedFailure)
    ));
    let snapshot = store.library_snapshot().await?;
    assert_eq!(snapshot.revision, Revision::new(3));
    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(snapshot.projects[0].summary.id, existing_id);
    assert_eq!(
        snapshot.sections.shared_records["stable"],
        br#"{"value":"before"}"#
    );
    assert!(snapshot.sections.assets.is_empty());
    assert_eq!(snapshot.provenance.len(), 1);
    assert_eq!(
        snapshot.settings["stable-setting"].value,
        json!({"value": "before"})
    );
    assert!(!snapshot.settings.contains_key("rolled-back-setting"));
    Ok(())
}

#[tokio::test]
async fn lifecycle_mutations_require_the_current_scenario_revision() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(35)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(id)?,
        })
        .await?;
    store
        .execute_command(id, Revision::INITIAL, RedoBranchPolicy::Reject, |current| {
            Ok(CommandWrite {
                document: set_marker(current.clone(), Some(1), UPDATED)?,
                journal: journal(json!({"marker": 1}), None, UPDATED)?,
                output: (),
            })
        })
        .await?;
    assert!(matches!(
        store
            .archive_project(id, Revision::INITIAL, timestamp(UPDATED)?)
            .await,
        Err(StoreError::Conflict { expected, actual })
            if expected == Revision::INITIAL && actual == Revision::new(1)
    ));
    assert!(store.get_project(id).await?.summary.archived_at.is_none());
    store
        .archive_project(id, Revision::new(1), timestamp(UPDATED)?)
        .await?;
    assert!(matches!(
        store.unarchive_project(id, Revision::INITIAL).await,
        Err(StoreError::Conflict { .. })
    ));
    assert!(matches!(
        store.delete_project(id, Revision::INITIAL).await,
        Err(StoreError::Conflict { .. })
    ));
    assert!(store.get_project(id).await?.summary.archived_at.is_some());
    store.unarchive_project(id, Revision::new(1)).await?;
    store.delete_project(id, Revision::new(1)).await?;
    assert!(matches!(
        store.get_project(id).await,
        Err(StoreError::ScenarioNotFound(found)) if found == id
    ));
    Ok(())
}

fn oversized_document(id: ScenarioId) -> Result<ScenarioDocument, Box<dyn Error>> {
    let mut oversized = document(id)?;
    let max_document_bytes = usize::try_from(MAX_SCENARIO_DOCUMENT_BYTES)?;
    oversized.extensions.insert(
        "oversized".to_owned(),
        Value::String("x".repeat(max_document_bytes)),
    );
    Ok(oversized)
}

#[tokio::test]
async fn authoritative_document_limit_covers_creation_commands_import_restore_and_snapshot_policy()
-> Result<(), Box<dyn Error>> {
    let interval = NonZeroU32::new(1).ok_or("nonzero interval")?;
    assert!(matches!(
        SnapshotPolicy::new(interval, MAX_SCENARIO_DOCUMENT_BYTES - 1, 3),
        Err(StoreError::InvalidSnapshotPolicy)
    ));
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let oversized_id = scenario_id(36)?;
    let legal_id = scenario_id(37)?;
    let imported_id = scenario_id(38)?;
    let mut oversized = oversized_document(oversized_id)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    assert!(matches!(
        store
            .create_project(NewProject {
                document: oversized.clone(),
            })
            .await,
        Err(StoreError::ScenarioDocumentTooLarge)
    ));
    store
        .create_project(NewProject {
            document: document(legal_id)?,
        })
        .await?;
    let mut command_document = oversized.clone();
    command_document.scenario_id = legal_id;
    assert!(matches!(
        store
            .execute_command(
                legal_id,
                Revision::INITIAL,
                RedoBranchPolicy::Reject,
                move |_| Ok(CommandWrite {
                    document: command_document,
                    journal: journal(json!({"oversized": true}), None, UPDATED)?,
                    output: (),
                }),
            )
            .await,
        Err(StoreError::ScenarioDocumentTooLarge)
    ));
    assert_eq!(
        store.get_project(legal_id).await?.summary.revision,
        Revision::INITIAL
    );
    oversized.scenario_id = imported_id;
    let staged = staged_import(
        Revision::new(1),
        vec![(Revision::new(5), oversized, StagedDisposition::Create)],
        timestamp(CREATED)?,
    );
    assert!(matches!(
        store
            .apply_staged_library(StagedLibraryApply::Import(staged), timestamp(LATER)?)
            .await,
        Err(StoreError::ScenarioDocumentTooLarge)
    ));
    assert!(matches!(
        store.get_project(imported_id).await,
        Err(StoreError::ScenarioNotFound(found)) if found == imported_id
    ));
    let restored_id = scenario_id(40)?;
    let restored_oversized = oversized_document(restored_id)?;
    let mut restore = staged_import(
        Revision::new(1),
        vec![(
            Revision::new(6),
            restored_oversized,
            StagedDisposition::Create,
        )],
        timestamp(CREATED)?,
    );
    restore.mode = RestoreMode::AddBackup;
    assert!(matches!(
        store
            .apply_staged_library(
                StagedLibraryApply::BackupRestore {
                    restore: StagedBackupRestore {
                        import: restore,
                        remove_scenario_ids: BTreeSet::new(),
                        authorization: RestoreAuthorization {
                            destructive_action_confirmed: false,
                            prospective_failure_receipt_token: None,
                            collision_plan_sha256: None,
                            safety_backup: SafetyBackupEvidence::NotRequired,
                        },
                    },
                    settings: BTreeMap::new(),
                },
                timestamp(LATER)?
            )
            .await,
        Err(StoreError::ScenarioDocumentTooLarge)
    ));
    assert!(matches!(
        store.get_project(restored_id).await,
        Err(StoreError::ScenarioNotFound(found)) if found == restored_id
    ));
    Ok(())
}
#[tokio::test]
async fn replace_revision_advances_past_a_newer_local_revision() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(39)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(id)?,
        })
        .await?;
    store
        .execute_command(id, Revision::INITIAL, RedoBranchPolicy::Reject, |current| {
            Ok(CommandWrite {
                document: set_marker(current.clone(), Some(1), UPDATED)?,
                journal: journal(json!({"marker": 1}), None, UPDATED)?,
                output: (),
            })
        })
        .await?;
    let staged = staged_import(
        Revision::new(2),
        vec![(Revision::new(2), document(id)?, StagedDisposition::Replace)],
        timestamp(CREATED)?,
    );
    store
        .apply_staged_library(StagedLibraryApply::Import(staged), timestamp(LATER)?)
        .await?;
    let project = store.get_project(id).await?;
    assert_eq!(project.summary.revision, Revision::new(2));
    let provenance = &store.library_snapshot().await?.provenance[0];
    assert_eq!(
        provenance.scenario_sources[0].source_revision,
        Revision::new(2)
    );
    Ok(())
}

fn aba_replace_restore(
    id: ScenarioId,
    removed_id: ScenarioId,
    source_document: ScenarioDocument,
) -> Result<StagedLibraryApply, Box<dyn Error>> {
    let mut restore = staged_import(
        Revision::new(3),
        vec![(
            Revision::new(7),
            set_marker(document(id)?, Some(7), LATER)?,
            StagedDisposition::Replace,
        )],
        timestamp(CREATED)?,
    );
    restore.mode = RestoreMode::ReplaceLibrary;
    restore.scenario_revisions.push(PortableScenario::current(
        Revision::new(6),
        source_document,
        BTreeSet::new(),
    ));
    restore.results.insert(
        "aba-result".to_owned(),
        serde_json::to_vec(&json!({
            "resultId": Uuid::now_v7(),
            "scenarioId": id,
            "scenarioRevision": 6,
            "result": {"score": 6}
        }))?,
    );
    Ok(StagedLibraryApply::BackupRestore {
        restore: StagedBackupRestore {
            import: restore,
            remove_scenario_ids: BTreeSet::from([id, removed_id]),
            authorization: RestoreAuthorization {
                destructive_action_confirmed: true,
                prospective_failure_receipt_token: None,
                collision_plan_sha256: Some("d".repeat(64)),
                safety_backup: SafetyBackupEvidence::Verified {
                    bundle_sha256: "verified-aba-backup".to_owned(),
                },
            },
        },
        settings: BTreeMap::new(),
    })
}

#[tokio::test]
// The ordered create/replace/delete/restart flow is the ABA contract under test.
#[allow(clippy::too_many_lines)]
async fn scenario_revision_high_water_prevents_aba_and_survives_restore_restart()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(106)?;
    let removed_id = scenario_id(107)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    let initial = staged_import(
        Revision::INITIAL,
        vec![
            (Revision::new(5), document(id)?, StagedDisposition::Create),
            (
                Revision::new(4),
                document(removed_id)?,
                StagedDisposition::Create,
            ),
        ],
        timestamp(CREATED)?,
    );
    store
        .apply_staged_library(StagedLibraryApply::Import(initial), timestamp(UPDATED)?)
        .await?;
    store.delete_project(id, Revision::new(5)).await?;
    assert_eq!(
        store.library_snapshot().await?.scenario_revision_high_water[&id],
        Revision::new(5)
    );

    let stale = staged_import(
        Revision::new(2),
        vec![(Revision::new(2), document(id)?, StagedDisposition::Create)],
        timestamp(CREATED)?,
    );
    assert!(matches!(
        store
            .apply_staged_library(StagedLibraryApply::Import(stale), timestamp(UPDATED)?)
            .await,
        Err(StoreError::InvalidStagedApply(_))
    ));
    assert!(matches!(
        store.get_project(id).await,
        Err(StoreError::ScenarioNotFound(found)) if found == id
    ));

    let mut unrepresented_source = staged_import(
        Revision::new(2),
        vec![(Revision::new(6), document(id)?, StagedDisposition::Create)],
        timestamp(CREATED)?,
    );
    unrepresented_source.scenarios[0].source_revision = Revision::new(2);
    unrepresented_source.results.insert(
        "missing-source-history-result".to_owned(),
        serde_json::to_vec(&json!({
            "resultId": Uuid::now_v7(),
            "scenarioId": id,
            "scenarioRevision": 2,
            "result": {"score": 2}
        }))?,
    );
    assert!(matches!(
        store
            .apply_staged_library(
                StagedLibraryApply::Import(unrepresented_source),
                timestamp(UPDATED)?,
            )
            .await,
        Err(StoreError::InvalidStagedApply(_))
    ));

    let mut target_six = staged_import(
        Revision::new(2),
        vec![(Revision::new(6), document(id)?, StagedDisposition::Create)],
        timestamp(CREATED)?,
    );
    target_six.scenarios[0].source_revision = Revision::new(2);
    target_six
        .scenario_revisions
        .push(PortableScenario::current(
            Revision::new(2),
            document(id)?,
            BTreeSet::new(),
        ));
    target_six.results.insert(
        "source-revision-result".to_owned(),
        serde_json::to_vec(&json!({
            "resultId": Uuid::now_v7(),
            "scenarioId": id,
            "scenarioRevision": 2,
            "result": {"score": 2}
        }))?,
    );
    store
        .apply_staged_library(StagedLibraryApply::Import(target_six), timestamp(UPDATED)?)
        .await?;
    assert_eq!(
        store.get_project(id).await?.summary.revision,
        Revision::new(6)
    );
    let snapshot = store.library_snapshot().await?;
    assert!(snapshot.provenance.iter().any(|provenance| {
        provenance
            .scenario_sources
            .iter()
            .any(|source| source.scenario_id == id && source.source_revision == Revision::new(2))
    }));
    assert!(snapshot.scenario_revisions.iter().any(|historical| {
        historical.scenario.document.scenario_id == id
            && historical.scenario.revision == Revision::new(2)
    }));
    drop(store);

    let (store, _) = SqliteScenarioStore::open(&path).await?;
    let snapshot = store.library_snapshot().await?;
    assert!(snapshot.provenance.iter().any(|provenance| {
        provenance
            .scenario_sources
            .iter()
            .any(|source| source.scenario_id == id && source.source_revision == Revision::new(2))
    }));
    let source_document = store.get_project(id).await?.document;
    store
        .apply_staged_library(
            aba_replace_restore(id, removed_id, source_document.clone())?,
            timestamp(LATER)?,
        )
        .await?;
    let snapshot = store.library_snapshot().await?;
    assert_eq!(snapshot.projects[0].summary.revision, Revision::new(7));
    assert_eq!(snapshot.scenario_revision_high_water[&id], Revision::new(7));
    assert_eq!(
        snapshot.scenario_revision_high_water[&removed_id],
        Revision::new(4)
    );
    assert_eq!(
        snapshot.scenario_revisions[0].scenario.document,
        source_document
    );
    drop(store);

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    let snapshot = reopened.library_snapshot().await?;
    assert_eq!(snapshot.scenario_revision_high_water[&id], Revision::new(7));
    assert_eq!(
        snapshot.scenario_revision_high_water[&removed_id],
        Revision::new(4)
    );
    assert!(matches!(
        reopened
            .execute_command(id, Revision::new(2), RedoBranchPolicy::Reject, |_| {
                Err::<CommandWrite<()>, StoreError>(StoreError::CommandApplication {
                    code: "must-not-run".to_owned(),
                    message: "stale callback ran".to_owned(),
                })
            })
            .await,
        Err(StoreError::Conflict { actual, .. }) if actual == Revision::new(7)
    ));
    Ok(())
}

#[tokio::test]
async fn no_result_source_revision_bumps_create_tombstone_and_replace_without_history()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(113)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(id)?,
        })
        .await?;
    store.delete_project(id, Revision::INITIAL).await?;

    let mut recreate = staged_import(
        Revision::new(2),
        vec![(Revision::new(2), document(id)?, StagedDisposition::Create)],
        timestamp(CREATED)?,
    );
    recreate.scenarios[0].source_revision = Revision::INITIAL;
    store
        .apply_staged_library(StagedLibraryApply::Import(recreate), timestamp(UPDATED)?)
        .await?;
    assert_eq!(
        store.get_project(id).await?.summary.revision,
        Revision::new(2)
    );
    assert!(
        store
            .library_snapshot()
            .await?
            .scenario_revisions
            .is_empty()
    );

    let mut replace = staged_import(
        Revision::new(3),
        vec![(Revision::new(3), document(id)?, StagedDisposition::Replace)],
        timestamp(CREATED)?,
    );
    replace.scenarios[0].source_revision = Revision::new(1);
    store
        .apply_staged_library(StagedLibraryApply::Import(replace), timestamp(LATER)?)
        .await?;
    let snapshot = store.library_snapshot().await?;
    assert_eq!(snapshot.projects[0].summary.revision, Revision::new(3));
    assert!(snapshot.scenario_revisions.is_empty());
    assert!(snapshot.provenance.iter().any(|provenance| {
        provenance
            .scenario_sources
            .iter()
            .any(|source| source.scenario_id == id && source.source_revision == Revision::new(1))
    }));
    drop(store);

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    let snapshot = reopened.library_snapshot().await?;
    assert_eq!(snapshot.projects[0].summary.revision, Revision::new(3));
    assert!(snapshot.scenario_revisions.is_empty());
    Ok(())
}

#[tokio::test]
async fn startup_backfills_revision_high_water_from_durable_history() -> Result<(), Box<dyn Error>>
{
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(108)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(id)?,
        })
        .await?;
    for (expected, marker) in [(Revision::INITIAL, 1), (Revision::new(1), 2)] {
        store
            .execute_command(id, expected, RedoBranchPolicy::Reject, move |current| {
                Ok(CommandWrite {
                    document: set_marker(current.clone(), Some(marker), LATER)?,
                    journal: journal(json!({"advance": marker}), None, LATER)?,
                    output: (),
                })
            })
            .await?;
    }
    drop(store);
    let connection = Connection::open(&path)?;
    connection.execute(
        "UPDATE scenario_revision_high_water SET highest_revision = 0 WHERE scenario_id = ?1",
        [id.to_string()],
    )?;
    drop(connection);

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    assert_eq!(
        reopened
            .library_snapshot()
            .await?
            .scenario_revision_high_water[&id],
        Revision::new(2)
    );
    Ok(())
}

#[tokio::test]
async fn duplicate_settings_and_online_backup_are_persistent() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let backup_path = directory.path().join("safety.sqlite3");
    let source_id = scenario_id(10)?;
    let copy_id = scenario_id(11)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(source_id)?,
        })
        .await?;
    let copy = store
        .duplicate_project(
            source_id,
            Revision::INITIAL,
            copy_id,
            BTreeMap::from([(source_id.as_uuid(), copy_id.as_uuid())]),
            "Clinic plan copy".to_owned(),
            timestamp(UPDATED)?,
        )
        .await?;
    assert_eq!(copy.summary.id, copy_id);
    assert_eq!(copy.summary.title, "Clinic plan copy");
    assert_eq!(copy.summary.revision, Revision::INITIAL);
    assert_eq!(copy.document.scenario_id, copy_id);
    assert_eq!(copy.document.metadata.title, "Clinic plan copy");

    store
        .set_setting(
            "appearance".to_owned(),
            json!({"theme": "system"}),
            timestamp(UPDATED)?,
        )
        .await?;
    let setting = store
        .get_setting::<Value>("appearance".to_owned())
        .await?
        .ok_or("setting was not persisted")?;
    assert_eq!(setting.value, json!({"theme": "system"}));
    store.safety_backup(&backup_path).await?;
    assert!(matches!(
        store.safety_backup(&backup_path).await,
        Err(StoreError::BackupDestinationExists)
    ));

    let backup = Connection::open(&backup_path)?;
    let copied_rows: u32 =
        backup.query_row("SELECT COUNT(*) FROM scenarios", [], |row| row.get(0))?;
    let setting_rows: u32 =
        backup.query_row("SELECT COUNT(*) FROM app_settings", [], |row| row.get(0))?;
    assert_eq!(copied_rows, 2);
    assert_eq!(setting_rows, 1);
    Ok(())
}

#[tokio::test]
async fn duplicate_refuses_missing_extra_and_occupied_identity_mappings_without_mutation()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let source_id = scenario_id(101)?;
    let occupied_id = scenario_id(102)?;
    let copy_id = scenario_id(103)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    for id in [source_id, occupied_id] {
        store
            .create_project(NewProject {
                document: document(id)?,
            })
            .await?;
    }
    let revision_before = store.library_snapshot().await?.revision;
    for (new_id, mapping) in [
        (copy_id, BTreeMap::new()),
        (
            copy_id,
            BTreeMap::from([
                (source_id.as_uuid(), copy_id.as_uuid()),
                (scenario_id(104)?.as_uuid(), scenario_id(105)?.as_uuid()),
            ]),
        ),
        (
            occupied_id,
            BTreeMap::from([(source_id.as_uuid(), occupied_id.as_uuid())]),
        ),
    ] {
        assert!(matches!(
            store
                .duplicate_project(
                    source_id,
                    Revision::INITIAL,
                    new_id,
                    mapping,
                    "Rejected copy".to_owned(),
                    timestamp(LATER)?,
                )
                .await,
            Err(StoreError::InvalidDuplicateMapping(_))
        ));
    }
    assert_eq!(store.library_snapshot().await?.revision, revision_before);
    assert!(matches!(
        store.get_project(copy_id).await,
        Err(StoreError::ScenarioNotFound(id)) if id == copy_id
    ));
    store.delete_project(occupied_id, Revision::INITIAL).await?;
    assert!(matches!(
        store
            .create_project(NewProject {
                document: document(occupied_id)?,
            })
            .await,
        Err(StoreError::ScenarioAlreadyExists(id)) if id == occupied_id
    ));
    assert!(matches!(
        store
            .duplicate_project(
                source_id,
                Revision::INITIAL,
                occupied_id,
                BTreeMap::from([(source_id.as_uuid(), occupied_id.as_uuid())]),
                "Rejected tombstone copy".to_owned(),
                timestamp(LATER)?,
            )
            .await,
        Err(StoreError::InvalidDuplicateMapping(_))
    ));
    Ok(())
}

fn assert_remapped_duplicate_graph(
    copy: &StoredProject,
    person: PersonId,
    rule: RuleId,
    semantic_id: Uuid,
    mapped_document_nonsemantic: Uuid,
    mapped_portable_nonsemantic: Uuid,
    unknown_nonsemantic: Uuid,
) -> Result<(), Box<dyn Error>> {
    let copied_domain = serde_json::to_value(&copy.document.domain)?;
    let copied_person = copied_domain["entities"]
        .as_object()
        .and_then(|values| values.keys().next())
        .ok_or("copied entity missing")?;
    let copied_rule = copied_domain["rules"]
        .as_object()
        .and_then(|values| values.keys().next())
        .ok_or("copied rule missing")?;
    assert_ne!(copied_person, &person.to_string());
    assert_ne!(copied_rule, &rule.to_string());
    assert_eq!(
        copied_domain["entities"][copied_person]["id"].as_str(),
        Some(copied_person.as_str())
    );
    assert_eq!(
        copied_domain["entities"][copied_person]["ruleId"].as_str(),
        Some(copied_rule.as_str())
    );
    assert_eq!(
        copied_domain["entities"][copied_person]["externalId"],
        person.to_string()
    );
    assert_eq!(
        copied_domain["entities"][copied_person]["note"],
        person.to_string()
    );
    assert_eq!(
        copied_domain["rules"][copied_rule]["participantId"].as_str(),
        Some(copied_person.as_str())
    );
    assert_eq!(
        copied_domain["rules"][copied_rule]["participantIds"][0].as_str(),
        Some(copied_person.as_str())
    );
    for (extension, mapped_nonsemantic) in [
        (
            &copy.document.extensions["vendor.document"],
            mapped_document_nonsemantic,
        ),
        (
            &copy.portable.extensions["vendor.display"],
            mapped_portable_nonsemantic,
        ),
    ] {
        assert_eq!(extension["scenarioId"], copy.summary.id.to_string());
        assert_eq!(
            extension["selectedEntityId"].as_str(),
            Some(copied_person.as_str())
        );
        assert_eq!(extension["externalId"], person.to_string());
        assert_eq!(extension["note"], person.to_string());
        assert_eq!(
            extension["definitions"][mapped_nonsemantic.to_string()]["id"],
            mapped_nonsemantic.to_string()
        );
        assert_eq!(
            extension["definitions"][unknown_nonsemantic.to_string()]["note"],
            "unknown definition key"
        );
    }
    let semantic = &copy.portable.semantic_extensions["vendor.semantic"];
    let copied_semantic_id = semantic["definitions"]
        .as_object()
        .and_then(|values| values.keys().next())
        .ok_or("copied semantic definition missing")?;
    assert_ne!(copied_semantic_id, &semantic_id.to_string());
    assert_eq!(
        semantic["definitions"][copied_semantic_id]["participantId"].as_str(),
        Some(copied_person.as_str())
    );
    Ok(())
}

#[tokio::test]
// One end-to-end restart fixture keeps the complete identity graph and metadata coupled.
#[allow(clippy::too_many_lines)]
async fn duplicate_remaps_owned_graph_and_preserves_portable_metadata_after_restart()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let source_id = scenario_id(90)?;
    let copy_id = scenario_id(91)?;
    let person = PersonId::from_uuid(Uuid::now_v7());
    let rule = RuleId::from_uuid(Uuid::now_v7());
    let semantic_id = Uuid::now_v7();
    let document_nonsemantic_owned = Uuid::now_v7();
    let mapped_document_nonsemantic = scenario_id(109)?.as_uuid();
    let portable_nonsemantic_owned = Uuid::now_v7();
    let mapped_portable_nonsemantic = scenario_id(112)?.as_uuid();
    let unknown_nonsemantic = Uuid::now_v7();
    let mut source_document = document(source_id)?;
    source_document.domain.entities.insert(
        person,
        json!({"id": person, "ruleId": rule, "externalId": person.to_string(), "note": person.to_string()}),
    );
    source_document.domain.rules.insert(
        rule,
        json!({"id": rule, "participantId": person, "participantIds": [person]}),
    );
    let document_nonsemantic = json!({
        "scenarioId": source_id,
        "selectedEntityId": person,
        "externalId": person,
        "note": person.to_string(),
        "definitions": {
            (document_nonsemantic_owned.to_string()): {
                "id": document_nonsemantic_owned,
                "selectedEntityId": person
            },
            (unknown_nonsemantic.to_string()): {
                "note": "unknown definition key"
            }
        }
    });
    source_document
        .extensions
        .insert("vendor.document".to_owned(), document_nonsemantic);
    let portable_nonsemantic = json!({
        "scenarioId": source_id,
        "selectedEntityId": person,
        "externalId": person,
        "note": person.to_string(),
        "definitions": {
            (portable_nonsemantic_owned.to_string()): {
                "id": portable_nonsemantic_owned,
                "selectedEntityId": person
            },
            (unknown_nonsemantic.to_string()): {
                "note": "unknown definition key"
            }
        }
    });
    let mut imported = staged_import(
        Revision::INITIAL,
        vec![(Revision::new(4), source_document, StagedDisposition::Create)],
        timestamp(CREATED)?,
    );
    imported.scenarios[0]
        .scenario
        .required_capabilities
        .insert(SemanticCapability {
            id: "vendor.semantic".to_owned(),
            version: 1,
        });
    imported.scenarios[0].scenario.semantic_extensions.insert(
        "vendor.semantic".to_owned(),
        json!({
            "definitions": {
                (semantic_id.to_string()): {
                    "id": semantic_id,
                    "participantId": person
                }
            }
        }),
    );
    imported.scenarios[0]
        .scenario
        .extensions
        .insert("vendor.display".to_owned(), portable_nonsemantic);
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .apply_staged_library(StagedLibraryApply::Import(imported), timestamp(UPDATED)?)
        .await?;

    let id_remap = BTreeMap::from([
        (source_id.as_uuid(), copy_id.as_uuid()),
        (person.as_uuid(), scenario_id(98)?.as_uuid()),
        (rule.as_uuid(), scenario_id(99)?.as_uuid()),
        (semantic_id, scenario_id(100)?.as_uuid()),
        (document_nonsemantic_owned, mapped_document_nonsemantic),
        (portable_nonsemantic_owned, mapped_portable_nonsemantic),
    ]);
    let copy = store
        .duplicate_project(
            source_id,
            Revision::new(4),
            copy_id,
            id_remap,
            "Graph copy".to_owned(),
            timestamp(LATER)?,
        )
        .await?;
    assert_remapped_duplicate_graph(
        &copy,
        person,
        rule,
        semantic_id,
        mapped_document_nonsemantic,
        mapped_portable_nonsemantic,
        unknown_nonsemantic,
    )?;
    drop(store);

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    let persisted = reopened.get_project(copy_id).await?;
    assert_eq!(persisted.summary.revision, copy.summary.revision);
    assert_eq!(persisted.document, copy.document);
    assert_eq!(persisted.portable, copy.portable);
    let mut exportable = PortableScenario::current(
        persisted.summary.revision,
        persisted.document,
        persisted.portable.required_capabilities,
    );
    exportable.semantic_extensions = persisted.portable.semantic_extensions;
    exportable.extensions = persisted.portable.extensions;
    validate_current_portable_scenario(&exportable)?;
    assert_eq!(
        serde_json::to_value(&exportable)?["extensions"]["vendor.display"],
        copy.portable.extensions["vendor.display"]
    );
    Ok(())
}

#[tokio::test]
async fn stale_duplicate_is_rejected_without_mutation_and_survives_reopen()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let source_id = scenario_id(12)?;
    let copy_id = scenario_id(13)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(source_id)?,
        })
        .await?;
    store
        .execute_command(
            source_id,
            Revision::INITIAL,
            RedoBranchPolicy::Reject,
            |current| {
                Ok(CommandWrite {
                    document: set_marker(current.clone(), Some(1), UPDATED)?,
                    journal: journal(json!({"marker": 1}), Some(json!({"marker": null})), UPDATED)?,
                    output: (),
                })
            },
        )
        .await?;
    let revision_before = store.library_snapshot().await?.revision;

    assert!(matches!(
        store
            .duplicate_project(
                source_id,
                Revision::INITIAL,
                copy_id,
                BTreeMap::from([(source_id.as_uuid(), copy_id.as_uuid())]),
                "Stale copy".to_owned(),
                timestamp(LATER)?,
            )
            .await,
        Err(StoreError::Conflict {
            expected: Revision::INITIAL,
            actual
        }) if actual == Revision::new(1)
    ));
    assert_eq!(store.library_snapshot().await?.revision, revision_before);
    assert!(matches!(
        store.get_project(copy_id).await,
        Err(StoreError::ScenarioNotFound(id)) if id == copy_id
    ));
    drop(store);

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    assert_eq!(reopened.library_snapshot().await?.revision, revision_before);
    assert_eq!(
        reopened.get_project(source_id).await?.summary.revision,
        Revision::new(1)
    );
    assert!(matches!(
        reopened.get_project(copy_id).await,
        Err(StoreError::ScenarioNotFound(id)) if id == copy_id
    ));
    Ok(())
}

#[tokio::test]
async fn stale_revision_is_rejected_before_callback_or_mutation() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(2)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(id)?,
        })
        .await?;

    store
        .execute_command(id, Revision::INITIAL, RedoBranchPolicy::Reject, |current| {
            Ok(CommandWrite {
                document: set_marker(current.clone(), Some(1), UPDATED)?,
                journal: journal(json!({"marker": 1}), Some(json!({"marker": null})), UPDATED)?,
                output: (),
            })
        })
        .await?;

    let callback_ran = Arc::new(AtomicBool::new(false));
    let callback_flag = Arc::clone(&callback_ran);
    let result = store
        .execute_command(
            id,
            Revision::INITIAL,
            RedoBranchPolicy::Reject,
            move |current| {
                callback_flag.store(true, Ordering::SeqCst);
                Ok(CommandWrite {
                    document: current.clone(),
                    journal: journal(json!({}), None, LATER)?,
                    output: (),
                })
            },
        )
        .await;
    assert!(matches!(
        result,
        Err(StoreError::Conflict { expected, actual })
            if expected == Revision::INITIAL && actual == Revision::new(1)
    ));
    assert!(!callback_ran.load(Ordering::SeqCst));
    assert_eq!(
        store.get_project(id).await?.summary.revision,
        Revision::new(1)
    );
    assert_eq!(store.history(id).await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn actor_self_drop_detaches_without_deadlock_and_allows_reopen() -> Result<(), Box<dyn Error>>
{
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(94)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(id)?,
        })
        .await?;
    let callback_store = store.clone();
    let task_store = store.clone();
    drop(store);
    let entered = Arc::new(std::sync::Barrier::new(2));
    let release = Arc::new(std::sync::Barrier::new(2));
    let callback_entered = Arc::clone(&entered);
    let callback_release = Arc::clone(&release);
    let (finished_sender, finished_receiver) = std::sync::mpsc::sync_channel(1);
    let task = tokio::spawn(async move {
        task_store
            .execute_command(
                id,
                Revision::INITIAL,
                RedoBranchPolicy::Reject,
                move |current| {
                    callback_entered.wait();
                    callback_release.wait();
                    let write = CommandWrite {
                        document: set_marker(current.clone(), Some(1), UPDATED)?,
                        journal: journal(
                            json!({"marker": 1}),
                            Some(json!({"marker": null})),
                            UPDATED,
                        )?,
                        output: (),
                    };
                    drop(callback_store);
                    finished_sender
                        .send(())
                        .map_err(|_| StoreError::ActorUnavailable)?;
                    Ok(write)
                },
            )
            .await
    });
    tokio::task::spawn_blocking(move || {
        entered.wait();
    })
    .await?;
    task.abort();
    let _aborted = task.await;
    release.wait();
    tokio::task::spawn_blocking(move || {
        finished_receiver.recv_timeout(std::time::Duration::from_secs(2))
    })
    .await??;

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    assert_eq!(
        reopened.get_project(id).await?.summary.revision,
        Revision::new(1)
    );
    Ok(())
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn document_write_failpoint_rolls_back_document_journal_revision_and_snapshot()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(3)?;
    let interval = NonZeroU32::new(1).ok_or("nonzero interval")?;
    let options = OpenOptions::new(SnapshotPolicy::new(
        interval,
        MAX_SCENARIO_DOCUMENT_BYTES,
        3,
    )?);
    let (store, _) = SqliteScenarioStore::open_with_options(&path, options).await?;
    let initial = document(id)?;
    store
        .create_project(NewProject {
            document: initial.clone(),
        })
        .await?;
    store.set_failpoint(Failpoint::AfterDocumentWrite)?;

    let result = store
        .execute_command(id, Revision::INITIAL, RedoBranchPolicy::Reject, |current| {
            Ok(CommandWrite {
                document: set_marker(current.clone(), Some(1), UPDATED)?,
                journal: journal(json!({"marker": 1}), Some(json!({"marker": null})), UPDATED)?,
                output: (),
            })
        })
        .await;
    assert!(matches!(result, Err(StoreError::InjectedFailure)));
    let persisted = store.get_project(id).await?;
    assert_eq!(persisted.summary.revision, Revision::INITIAL);
    assert_eq!(persisted.document, initial);
    assert!(store.history(id).await?.is_empty());

    let connection = Connection::open(&path)?;
    let snapshots: u32 = connection.query_row(
        "SELECT COUNT(*) FROM scenario_snapshots WHERE scenario_id = ?1",
        [id.to_string()],
        |row| row.get(0),
    )?;
    assert_eq!(snapshots, 0);
    Ok(())
}

#[tokio::test]
async fn undo_and_redo_survive_restart_and_branch_truncation_is_explicit()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(4)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(id)?,
        })
        .await?;
    store
        .execute_command(id, Revision::INITIAL, RedoBranchPolicy::Reject, |current| {
            Ok(CommandWrite {
                document: set_marker(current.clone(), Some(1), UPDATED)?,
                journal: journal(json!({"marker": 1}), Some(json!({"marker": null})), UPDATED)?,
                output: (),
            })
        })
        .await?;
    let initial_document_timestamp = timestamp(CREATED)?;
    store
        .undo(id, Revision::new(1), timestamp(LATER)?, move |history| {
            assert_eq!(
                history.target_document_updated_at,
                initial_document_timestamp
            );
            let target = history.target_document_updated_at.to_string();
            Ok((set_marker(history.document, None, &target)?, ()))
        })
        .await?;
    assert_eq!(store.get_project(id).await?.document, document(id)?);
    drop(store);

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    let command_document_timestamp = timestamp(UPDATED)?;
    reopened
        .redo(id, Revision::new(2), timestamp(LATER)?, move |history| {
            assert_eq!(
                history.target_document_updated_at,
                command_document_timestamp
            );
            let target = history.target_document_updated_at.to_string();
            Ok((set_marker(history.document, Some(1), &target)?, ()))
        })
        .await?;
    let initial_document_timestamp = timestamp(CREATED)?;
    reopened
        .undo(id, Revision::new(3), timestamp(LATER)?, move |history| {
            assert_eq!(
                history.target_document_updated_at,
                initial_document_timestamp
            );
            let target = history.target_document_updated_at.to_string();
            Ok((set_marker(history.document, None, &target)?, ()))
        })
        .await?;

    let rejected = reopened
        .execute_command(id, Revision::new(4), RedoBranchPolicy::Reject, |current| {
            Ok(CommandWrite {
                document: set_marker(current.clone(), Some(2), LATER)?,
                journal: journal(json!({"marker": 2}), Some(json!({"marker": null})), LATER)?,
                output: (),
            })
        })
        .await;
    assert!(matches!(
        rejected,
        Err(StoreError::RedoBranchRequiresTruncation)
    ));
    reopened
        .execute_command(
            id,
            Revision::new(4),
            RedoBranchPolicy::Truncate,
            |current| {
                Ok(CommandWrite {
                    document: set_marker(current.clone(), Some(2), LATER)?,
                    journal: journal(json!({"marker": 2}), Some(json!({"marker": null})), LATER)?,
                    output: (),
                })
            },
        )
        .await?;
    assert_eq!(reopened.history(id).await?.len(), 1);
    assert!(matches!(
        reopened
            .redo(id, Revision::new(5), timestamp(LATER)?, |history| Ok((
                history.document,
                ()
            )))
            .await,
        Err(StoreError::NoRedo)
    ));
    Ok(())
}

async fn seed_scenario_referencing_supplemental(
    store: &SqliteScenarioStore,
    id: ScenarioId,
) -> Result<(), Box<dyn Error>> {
    let mut supplemental = staged_import(Revision::new(2), Vec::new(), timestamp(CREATED)?);
    supplemental.results.insert(
        "result-referencing".to_owned(),
        serde_json::to_vec(&json!({
            "resultId": Uuid::now_v7(),
            "scenarioId": id,
            "scenarioRevision": 1,
            "result": {"score": 7}
        }))?,
    );
    supplemental.shared_records.insert(
        "shared-referencing".to_owned(),
        serde_json::to_vec(&json!({"scenarioId": id, "value": 1}))?,
    );
    supplemental.shared_records.insert(
        "shared-unrelated".to_owned(),
        serde_json::to_vec(&json!({"externalId": id.to_string()}))?,
    );
    supplemental.preferences.insert(
        "preference-referencing".to_owned(),
        serde_json::to_vec(&json!({"scenarioId": id, "value": 2}))?,
    );
    supplemental.preferences.insert(
        "preference-unrelated".to_owned(),
        serde_json::to_vec(&json!({"prose": format!("scenario {id}")}))?,
    );
    supplemental.assets.insert(
        "shared-asset".to_owned(),
        PortableAsset {
            bytes: b"shared inert bytes".to_vec(),
            media_type: "text/plain".to_owned(),
            redistribution_permitted: false,
        },
    );
    store
        .apply_staged_library(StagedLibraryApply::Import(supplemental), timestamp(LATER)?)
        .await?;
    Ok(())
}

#[tokio::test]
async fn deleting_project_cascades_owned_rows_and_referencing_supplemental()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(5)?;
    let interval = NonZeroU32::new(1).ok_or("nonzero interval")?;
    let (store, _) = SqliteScenarioStore::open_with_options(
        &path,
        OpenOptions::new(SnapshotPolicy::new(
            interval,
            MAX_SCENARIO_DOCUMENT_BYTES,
            3,
        )?),
    )
    .await?;
    store
        .create_project(NewProject {
            document: document(id)?,
        })
        .await?;
    store
        .execute_command(id, Revision::INITIAL, RedoBranchPolicy::Reject, |current| {
            Ok(CommandWrite {
                document: set_marker(current.clone(), Some(1), UPDATED)?,
                journal: journal(json!({"marker": 1}), Some(json!({"marker": null})), UPDATED)?,
                output: (),
            })
        })
        .await?;
    seed_scenario_referencing_supplemental(&store, id).await?;

    let connection = Connection::open(&path)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.execute(
        "INSERT INTO solve_runs (id, scenario_id, scenario_revision, input_hash, backend_id, backend_version, status, options_json, started_at) VALUES ('run-1', ?1, 1, 'hash', 'backend', '1', 'completed', '{}', ?2)",
        params![id.to_string(), CREATED],
    )?;
    connection.execute(
        "INSERT INTO solutions (id, solve_run_id, scenario_id, scenario_revision, status, accepted, normalized_solution_json, score_json, verification_report_json, created_at) VALUES ('solution-1', 'run-1', ?1, 1, 'verified', 1, '{}', '{}', '{}', ?2)",
        params![id.to_string(), CREATED],
    )?;
    connection.execute(
        "INSERT INTO ai_conversations (id, scenario_id, title, provider_id, model_id, created_at, updated_at) VALUES ('conversation-1', ?1, 'Conversation', 'provider', 'model', ?2, ?2)",
        params![id.to_string(), CREATED],
    )?;
    connection.execute(
        "INSERT INTO ai_messages (id, conversation_id, role, content_json, created_at) VALUES ('message-1', 'conversation-1', 'user', '{}', ?1)",
        [CREATED],
    )?;
    drop(connection);

    store.delete_project(id, Revision::new(1)).await?;
    let supplemental = store.library_snapshot().await?.sections;
    assert!(!supplemental.results.contains_key("result-referencing"));
    assert!(
        !supplemental
            .shared_records
            .contains_key("shared-referencing")
    );
    assert!(supplemental.shared_records.contains_key("shared-unrelated"));
    assert!(
        !supplemental
            .preferences
            .contains_key("preference-referencing")
    );
    assert!(
        supplemental
            .preferences
            .contains_key("preference-unrelated")
    );
    assert_eq!(
        supplemental.assets["shared-asset"],
        PortableAsset {
            bytes: b"shared inert bytes".to_vec(),
            media_type: "text/plain".to_owned(),
            redistribution_permitted: false,
        }
    );
    let connection = Connection::open(&path)?;
    for table in [
        "scenarios",
        "scenario_snapshots",
        "retained_scenario_revisions",
        "command_journal",
        "scenario_history_state",
        "solve_runs",
        "solutions",
        "ai_conversations",
        "ai_messages",
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let count: u32 = connection.query_row(&sql, [], |row| row.get(0))?;
        assert_eq!(count, 0, "rows remained in {table}");
    }
    Ok(())
}
#[cfg(unix)]
#[test]
fn private_application_directory_is_owner_only_and_refuses_symlinks() -> Result<(), Box<dyn Error>>
{
    use std::os::unix::fs::{PermissionsExt, symlink};

    let directory = tempdir()?;
    let private = directory.path().join("application").join("backups");
    ensure_private_application_directory(&private)?;
    assert_eq!(
        std::fs::symlink_metadata(&private)?.permissions().mode() & 0o777,
        0o700
    );
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o777))?;
    ensure_private_application_directory(&private)?;
    assert_eq!(
        std::fs::symlink_metadata(&private)?.permissions().mode() & 0o777,
        0o700
    );

    let target = directory.path().join("attacker-controlled");
    std::fs::create_dir(&target)?;
    let linked = directory.path().join("linked-backups");
    symlink(&target, &linked)?;
    assert!(matches!(
        ensure_private_application_directory(&linked),
        Err(StoreError::PrivatePath(_))
    ));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn authoritative_database_and_safety_backup_are_private_and_refuse_indirection()
-> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;

    let directory = tempdir()?;
    let data_dir = directory.path().join("permissive-data");
    std::fs::create_dir(&data_dir)?;
    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o777))?;
    let database = data_dir.join("library.sqlite3");
    let (store, _) = SqliteScenarioStore::open(&database).await?;
    assert_eq!(
        std::fs::metadata(&data_dir)?.permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&database)?.permissions().mode() & 0o777,
        0o600
    );
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", database.display()));
        if sidecar.exists() {
            assert_eq!(
                std::fs::metadata(sidecar)?.permissions().mode() & 0o777,
                0o600
            );
        }
    }

    let backup_dir = directory.path().join("permissive-backups");
    std::fs::create_dir(&backup_dir)?;
    std::fs::set_permissions(&backup_dir, std::fs::Permissions::from_mode(0o777))?;
    let backup = backup_dir.join("safety.sqlite3");
    store.safety_backup(&backup).await?;
    assert_eq!(
        std::fs::metadata(&backup_dir)?.permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&backup)?.permissions().mode() & 0o777,
        0o600
    );

    let target = directory.path().join("attacker.sqlite3");
    std::fs::write(&target, b"not a database")?;
    let linked_database = data_dir.join("linked.sqlite3");
    symlink(&target, &linked_database)?;
    assert!(matches!(
        SqliteScenarioStore::open(&linked_database).await,
        Err(StoreError::PrivatePath(_))
    ));
    let linked_backup = backup_dir.join("linked-backup.sqlite3");
    symlink(&target, &linked_backup)?;
    assert!(matches!(
        store.safety_backup(&linked_backup).await,
        Err(StoreError::PrivatePath(_))
    ));
    let hard_linked_database = data_dir.join("hard-linked.sqlite3");
    std::fs::hard_link(&target, &hard_linked_database)?;
    assert!(matches!(
        SqliteScenarioStore::open(&hard_linked_database).await,
        Err(StoreError::PrivatePath(_))
    ));

    let actual_parent = directory.path().join("actual-private-parent");
    std::fs::create_dir(&actual_parent)?;
    let linked_parent = directory.path().join("linked-parent");
    symlink(&actual_parent, &linked_parent)?;
    assert!(matches!(
        SqliteScenarioStore::open(linked_parent.join("library.sqlite3")).await,
        Err(StoreError::PrivatePath(_))
    ));

    let socket_path = data_dir.join("special.sqlite3");
    let _socket = UnixListener::bind(&socket_path)?;
    assert!(matches!(
        SqliteScenarioStore::open(&socket_path).await,
        Err(StoreError::PrivatePath(_))
    ));
    Ok(())
}

#[cfg(windows)]
#[tokio::test]
async fn windows_platform_app_data_supports_private_store_creation() -> Result<(), Box<dyn Error>> {
    let app_data = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .ok_or("APPDATA is not configured")?;
    for precreate in [false, true] {
        let data_dir = app_data.join(format!("eutheto-store-test-{}", Uuid::now_v7()));
        if precreate {
            std::fs::create_dir(&data_dir)?;
        }
        let database = data_dir.join("library.sqlite3");
        let (store, _) = SqliteScenarioStore::open(&database)
            .await
            .map_err(|error| format!("platform app-data store initialization failed: {error:?}"))?;
        drop(store);
        std::fs::remove_dir_all(data_dir)?;
    }
    Ok(())
}

#[cfg(windows)]
#[tokio::test]
async fn windows_authoritative_paths_apply_private_acls_and_refuse_links()
-> Result<(), Box<dyn Error>> {
    use std::os::windows::fs::symlink_file;

    let directory = tempdir()?;
    let data_dir = directory.path().join("data");
    std::fs::create_dir(&data_dir)?;
    let database = data_dir.join("library.sqlite3");
    let (store, _) = SqliteScenarioStore::open(&database).await?;
    let backup_dir = directory.path().join("backups");
    std::fs::create_dir(&backup_dir)?;
    let backup = backup_dir.join("safety.sqlite3");
    store.safety_backup(&backup).await?;

    let target = directory.path().join("attacker.sqlite3");
    std::fs::write(&target, b"not a database")?;
    let hard_linked = data_dir.join("hard-linked.sqlite3");
    std::fs::hard_link(&target, &hard_linked)?;
    assert!(matches!(
        SqliteScenarioStore::open(&hard_linked).await,
        Err(StoreError::PrivatePath(_))
    ));
    let linked = data_dir.join("linked.sqlite3");
    if symlink_file(&target, &linked).is_ok() {
        assert!(matches!(
            SqliteScenarioStore::open(&linked).await,
            Err(StoreError::PrivatePath(_))
        ));
    }
    Ok(())
}

#[cfg(debug_assertions)]
async fn seed_and_advance_retained_revision(
    store: &SqliteScenarioStore,
    id: ScenarioId,
    revision_seven: &ScenarioDocument,
) -> Result<(), Box<dyn Error>> {
    let mut initial = staged_import(
        Revision::INITIAL,
        vec![(
            Revision::new(7),
            revision_seven.clone(),
            StagedDisposition::Create,
        )],
        timestamp(CREATED)?,
    );
    initial.results.insert(
        "immutable-result".to_owned(),
        serde_json::to_vec(&json!({
            "resultId": Uuid::now_v7(),
            "scenarioId": id,
            "scenarioRevision": 7,
            "result": {"score": 7}
        }))?,
    );
    store
        .apply_staged_library(StagedLibraryApply::Import(initial), timestamp(LATER)?)
        .await?;
    assert!(
        store
            .library_snapshot()
            .await?
            .scenario_revisions
            .is_empty()
    );
    store
        .execute_command(id, Revision::new(7), RedoBranchPolicy::Reject, |current| {
            Ok(CommandWrite {
                document: set_marker(current.clone(), Some(8), UPDATED)?,
                journal: journal(json!({"marker": 8}), Some(json!({"marker": 7})), UPDATED)?,
                output: (),
            })
        })
        .await?;
    let advanced = store.library_snapshot().await?;
    assert_eq!(advanced.projects[0].summary.revision, Revision::new(8));
    assert_eq!(advanced.scenario_revisions.len(), 1);
    assert_eq!(
        advanced.scenario_revisions[0].scenario.document,
        *revision_seven
    );
    assert_eq!(
        advanced.scenario_revisions[0].scenario.revision,
        Revision::new(7)
    );
    Ok(())
}

#[cfg(debug_assertions)]
fn exact_revision_restore(
    id: ScenarioId,
    revision_seven: &ScenarioDocument,
) -> Result<StagedLibraryApply, Box<dyn Error>> {
    let mut restore = staged_import(
        Revision::new(2),
        vec![(
            Revision::new(20),
            set_marker(document(id)?, Some(20), LATER)?,
            StagedDisposition::Replace,
        )],
        timestamp(CREATED)?,
    );
    restore.mode = RestoreMode::ReplaceLibrary;
    restore.scenario_revisions.push(PortableScenario::current(
        Revision::new(7),
        revision_seven.clone(),
        BTreeSet::new(),
    ));
    restore.results.insert(
        "immutable-result".to_owned(),
        serde_json::to_vec(&json!({
            "resultId": Uuid::now_v7(),
            "scenarioId": id,
            "scenarioRevision": 7,
            "result": {"score": 7}
        }))?,
    );
    Ok(StagedLibraryApply::BackupRestore {
        restore: StagedBackupRestore {
            import: restore,
            remove_scenario_ids: BTreeSet::from([id]),
            authorization: RestoreAuthorization {
                destructive_action_confirmed: true,
                prospective_failure_receipt_token: None,
                collision_plan_sha256: Some("a".repeat(64)),
                safety_backup: SafetyBackupEvidence::Verified {
                    bundle_sha256: "verified-exact-revision".to_owned(),
                },
            },
        },
        settings: BTreeMap::new(),
    })
}

#[cfg(debug_assertions)]
async fn assert_atomic_restore_preserves_exact_revision(
    store: &SqliteScenarioStore,
    id: ScenarioId,
    staged_restore: StagedLibraryApply,
    revision_seven: &ScenarioDocument,
) -> Result<(), Box<dyn Error>> {
    store.set_failpoint(Failpoint::AfterSupplementalWrite)?;
    assert!(matches!(
        store
            .apply_staged_library(staged_restore.clone(), timestamp(LATER)?)
            .await,
        Err(StoreError::InjectedFailure)
    ));
    let after_failure = store.library_snapshot().await?;
    assert_eq!(after_failure.projects[0].summary.revision, Revision::new(8));
    assert_eq!(
        after_failure.scenario_revision_high_water[&id],
        Revision::new(8)
    );
    assert_eq!(
        after_failure.scenario_revisions[0].scenario.document,
        *revision_seven
    );
    store
        .apply_staged_library(staged_restore, timestamp(LATER)?)
        .await?;
    let restored = store.library_snapshot().await?;
    assert_eq!(restored.projects[0].summary.revision, Revision::new(20));
    assert_eq!(
        restored.scenario_revision_high_water[&id],
        Revision::new(20)
    );
    assert_eq!(restored.scenario_revisions.len(), 1);
    assert_eq!(
        restored.scenario_revisions[0].scenario.document,
        *revision_seven
    );
    Ok(())
}

#[cfg(debug_assertions)]
async fn replace_result_and_cleanup_retained_revision(
    store: &SqliteScenarioStore,
    id: ScenarioId,
) -> Result<(), Box<dyn Error>> {
    let mut replace_result = staged_import(Revision::new(3), Vec::new(), timestamp(CREATED)?);
    replace_result.results.insert(
        "immutable-result".to_owned(),
        serde_json::to_vec(&json!({
            "resultId": Uuid::now_v7(),
            "scenarioId": id,
            "scenarioRevision": 20,
            "result": {"score": 20}
        }))?,
    );
    replace_result
        .supplemental_replacements
        .insert(SupplementalIdentity {
            section: SupplementalSectionKind::Results,
            key: "immutable-result".to_owned(),
        });
    store
        .apply_staged_library(
            StagedLibraryApply::Import(replace_result),
            timestamp(LATER)?,
        )
        .await?;
    assert!(
        store
            .library_snapshot()
            .await?
            .scenario_revisions
            .is_empty()
    );
    store.delete_project(id, Revision::new(20)).await?;
    Ok(())
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn retained_exact_revision_survives_advance_restart_atomic_restore_and_cleanup()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(80)?;
    let revision_seven = set_marker(document(id)?, Some(7), CREATED)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    seed_and_advance_retained_revision(&store, id, &revision_seven).await?;
    drop(store);

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    assert_eq!(
        reopened.library_snapshot().await?.scenario_revisions[0]
            .scenario
            .document,
        revision_seven
    );
    let staged_restore = exact_revision_restore(id, &revision_seven)?;
    assert_atomic_restore_preserves_exact_revision(&reopened, id, staged_restore, &revision_seven)
        .await?;
    drop(reopened);

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    assert_eq!(
        reopened.library_snapshot().await?.scenario_revisions[0]
            .scenario
            .document,
        revision_seven
    );
    replace_result_and_cleanup_retained_revision(&reopened, id).await?;
    drop(reopened);

    let connection = Connection::open(&path)?;
    let retained_count: u32 = connection.query_row(
        "SELECT COUNT(*) FROM retained_scenario_revisions",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(retained_count, 0);
    Ok(())
}

#[tokio::test]
async fn all_skip_apply_is_a_true_no_effect() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    let mut skipped = staged_import(Revision::INITIAL, Vec::new(), timestamp(CREATED)?);
    skipped
        .manifest_extensions
        .insert("vendor.skipped".to_owned(), json!({"ignored": true}));
    skipped
        .nonsemantic_extensions
        .insert("vendor.skipped".to_owned());
    let outcome = store
        .apply_staged_library(StagedLibraryApply::Import(skipped), timestamp(LATER)?)
        .await?;
    assert_eq!(outcome.library_revision, Revision::INITIAL);
    assert_eq!(outcome.created, 0);
    assert_eq!(outcome.replaced, 0);
    assert_eq!(outcome.removed, 0);
    let snapshot = store.library_snapshot().await?;
    assert_eq!(snapshot.revision, Revision::INITIAL);
    assert!(snapshot.provenance.is_empty());
    assert!(snapshot.manifest_extensions.is_empty());
    assert!(snapshot.nonsemantic_extensions.is_empty());
    Ok(())
}

#[tokio::test]
async fn oversized_provenance_refuses_the_entire_transaction() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let existing_id = scenario_id(81)?;
    let imported_id = scenario_id(82)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: document(existing_id)?,
        })
        .await?;
    let mut staged = staged_import(
        Revision::new(1),
        vec![(
            Revision::new(5),
            document(imported_id)?,
            StagedDisposition::Create,
        )],
        timestamp(CREATED)?,
    );
    staged.provenance.source_file_sha256 = "x".repeat(4 * 1024 * 1024 + 1);
    assert!(matches!(
        store
            .apply_staged_library(StagedLibraryApply::Import(staged), timestamp(LATER)?)
            .await,
        Err(StoreError::InvalidStagedApply(_))
    ));
    let snapshot = store.library_snapshot().await?;
    assert_eq!(snapshot.revision, Revision::new(1));
    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(snapshot.projects[0].summary.id, existing_id);
    assert!(snapshot.provenance.is_empty());
    Ok(())
}

#[tokio::test]
async fn provenance_pruning_is_bounded_and_deterministic() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    drop(store);
    let connection = Connection::open(&path)?;
    for index in 0..130 {
        connection.execute(
            "INSERT INTO portable_import_provenance (source_bundle_id, source_application_json, original_format_version, original_schema_version, source_file_sha256, applied_migrations_json, binding_json, scenario_sources_json, source_created_at, applied_at) VALUES (?1, ?2, 1, 1, ?3, '[]', ?4, '[]', ?5, ?6)",
            params![
                BundleId::from_uuid(Uuid::now_v7()).to_string(),
                r#"{"name":"seed","version":"1"}"#,
                format!("seed-{index}"),
                r#"{"fileSha256":"seed","optionsSha256":"seed","localLibraryRevision":0,"formatVersion":1,"schemaVersion":1}"#,
                CREATED,
                UPDATED,
            ],
        )?;
    }
    drop(connection);

    let (store, _) = SqliteScenarioStore::open(&path).await?;
    let mut effect = staged_import(Revision::INITIAL, Vec::new(), timestamp(CREATED)?);
    effect
        .shared_records
        .insert("retained".to_owned(), br#"{"value":true}"#.to_vec());
    store
        .apply_staged_library(StagedLibraryApply::Import(effect), timestamp(LATER)?)
        .await?;
    let provenance = store.library_snapshot().await?.provenance;
    assert_eq!(provenance.len(), 128);
    assert_eq!(provenance[0].source_file_sha256, "seed-3");
    assert_eq!(
        provenance
            .last()
            .map(|entry| entry.source_file_sha256.as_str()),
        Some("staged-test")
    );
    let latest = provenance.last().ok_or("new provenance row missing")?;
    assert_eq!(latest.source_created_at, timestamp(CREATED)?);
    assert_eq!(latest.applied_at, timestamp(LATER)?);
    drop(store);
    let connection = Connection::open(&path)?;
    let bytes: i64 = connection.query_row(
        "SELECT COALESCE(SUM(16 + length(CAST(source_bundle_id AS BLOB)) + length(CAST(source_application_json AS BLOB)) + length(CAST(source_file_sha256 AS BLOB)) + length(CAST(applied_migrations_json AS BLOB)) + length(CAST(binding_json AS BLOB)) + length(CAST(scenario_sources_json AS BLOB)) + length(CAST(source_created_at AS BLOB)) + length(CAST(applied_at AS BLOB))), 0) FROM portable_import_provenance",
        [],
        |row| row.get(0),
    )?;
    assert!(bytes <= 4 * 1024 * 1024);
    Ok(())
}

#[tokio::test]
async fn provenance_pruning_counts_multibyte_utf8_bytes() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    drop(store);
    let connection = Connection::open(&path)?;
    let source_application = serde_json::to_string(&ApplicationMetadata {
        name: "é".repeat(500_000),
        version: "1".to_owned(),
    })?;
    for index in 0..5 {
        connection.execute(
            "INSERT INTO portable_import_provenance (source_bundle_id, source_application_json, original_format_version, original_schema_version, source_file_sha256, applied_migrations_json, binding_json, scenario_sources_json, source_created_at, applied_at) VALUES (?1, ?2, 1, 1, ?3, '[]', ?4, '[]', ?5, ?6)",
            params![
                BundleId::from_uuid(Uuid::now_v7()).to_string(),
                source_application,
                format!("multibyte-{index}"),
                r#"{"fileSha256":"seed","optionsSha256":"seed","localLibraryRevision":0,"formatVersion":1,"schemaVersion":1}"#,
                CREATED,
                UPDATED,
            ],
        )?;
    }
    drop(connection);

    let (store, _) = SqliteScenarioStore::open(&path).await?;
    let mut effect = staged_import(Revision::INITIAL, Vec::new(), timestamp(CREATED)?);
    effect
        .shared_records
        .insert("retained".to_owned(), br#"{"value":true}"#.to_vec());
    store
        .apply_staged_library(StagedLibraryApply::Import(effect), timestamp(LATER)?)
        .await?;
    drop(store);

    let connection = Connection::open(&path)?;
    let bytes: i64 = connection.query_row(
        "SELECT COALESCE(SUM(16 + length(CAST(source_bundle_id AS BLOB)) + length(CAST(source_application_json AS BLOB)) + length(CAST(source_file_sha256 AS BLOB)) + length(CAST(applied_migrations_json AS BLOB)) + length(CAST(binding_json AS BLOB)) + length(CAST(scenario_sources_json AS BLOB)) + length(CAST(source_created_at AS BLOB)) + length(CAST(applied_at AS BLOB))), 0) FROM portable_import_provenance",
        [],
        |row| row.get(0),
    )?;
    assert!(bytes <= 4 * 1024 * 1024);
    Ok(())
}

#[tokio::test]
async fn results_excluded_replace_retains_the_exact_local_source_revision_after_restart()
-> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let id = scenario_id(83)?;
    let local_revision = set_marker(document(id)?, Some(0), CREATED)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: local_revision.clone(),
        })
        .await?;
    let mut result = staged_import(Revision::new(1), Vec::new(), timestamp(CREATED)?);
    result.results.insert(
        "local-result".to_owned(),
        serde_json::to_vec(&json!({
            "resultId": Uuid::now_v7(),
            "scenarioId": id,
            "scenarioRevision": 0,
            "result": {"score": 1}
        }))?,
    );
    store
        .apply_staged_library(StagedLibraryApply::Import(result), timestamp(LATER)?)
        .await?;

    let replacement_document = set_marker(document(id)?, Some(5), LATER)?;
    let replacement = staged_import(
        Revision::new(2),
        vec![(
            Revision::new(5),
            replacement_document,
            StagedDisposition::Replace,
        )],
        timestamp(CREATED)?,
    );
    store
        .apply_staged_library(StagedLibraryApply::Import(replacement), timestamp(LATER)?)
        .await?;
    let snapshot = store.library_snapshot().await?;
    assert_eq!(snapshot.projects[0].summary.revision, Revision::new(5));
    assert_eq!(snapshot.scenario_revisions.len(), 1);
    assert_eq!(
        snapshot.scenario_revisions[0].scenario.document,
        local_revision
    );
    assert!(snapshot.sections.results.contains_key("local-result"));
    drop(store);

    let (reopened, _) = SqliteScenarioStore::open(&path).await?;
    let snapshot = reopened.library_snapshot().await?;
    assert_eq!(snapshot.scenario_revisions.len(), 1);
    assert_eq!(
        snapshot.scenario_revisions[0].scenario.document,
        local_revision
    );
    assert!(snapshot.sections.results.contains_key("local-result"));
    Ok(())
}

#[tokio::test]
async fn persisted_raw_secret_sentinel_is_rejected_on_snapshot_load() -> Result<(), Box<dyn Error>>
{
    let directory = tempdir()?;
    for (index, field) in [
        "secret",
        "providerApiKey",
        "oauthCredentialId",
        "apiKeyHandle",
        "apiKeyReference",
        "vendor.providerApiKey",
        "vendor:oauthCredentialId",
        "vendor/apiKeyHandle",
    ]
    .into_iter()
    .enumerate()
    {
        let path = directory.path().join(format!("sentinel-{index}.sqlite3"));
        let (store, _) = SqliteScenarioStore::open(&path).await?;
        drop(store);
        let connection = Connection::open(&path)?;
        connection.execute(
            "INSERT INTO portable_sections (section, key, value) VALUES ('shared_records', 'sentinel', ?1)",
            [serde_json::to_vec(&json!({(field): "raw-secret-sentinel"}))?],
        )?;
        drop(connection);
        let (store, _) = SqliteScenarioStore::open(&path).await?;
        let metadata = store.library_metadata_snapshot().await?;
        assert_eq!(metadata.revision, Revision::INITIAL);
        assert_eq!(metadata.scenario_count, 0);
        assert!(matches!(
            store.library_snapshot().await,
            Err(StoreError::Integrity(_))
        ));
    }

    let safe_path = directory.path().join("safe-substring.sqlite3");
    let safe_value = serde_json::to_vec(&json!({"hockeyScore": 7}))?;
    let (store, _) = SqliteScenarioStore::open(&safe_path).await?;
    drop(store);
    let connection = Connection::open(&safe_path)?;
    connection.execute(
        "INSERT INTO portable_sections (section, key, value) VALUES ('shared_records', 'safe', ?1)",
        [&safe_value],
    )?;
    drop(connection);
    let (store, _) = SqliteScenarioStore::open(&safe_path).await?;
    assert_eq!(
        store.library_snapshot().await?.sections.shared_records["safe"],
        safe_value
    );
    Ok(())
}

#[tokio::test]
async fn startup_applies_required_pragmas_schema_and_indexes() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let (store, outcome) = SqliteScenarioStore::open(&path).await?;
    assert_eq!(outcome.schema_version, 1);
    let diagnostics = store.diagnostics().await?;
    assert!(diagnostics.foreign_keys);
    assert_eq!(diagnostics.journal_mode, "wal");
    assert_eq!(diagnostics.synchronous, 1);
    assert_eq!(diagnostics.busy_timeout_ms, 5_000);
    assert!(!diagnostics.trusted_schema);
    assert_eq!(diagnostics.schema_version, 1);
    for table in [
        "app_metadata",
        "scenarios",
        "scenario_snapshots",
        "retained_scenario_revisions",
        "command_journal",
        "scenario_history_state",
        "solve_runs",
        "solutions",
        "app_settings",
        "portable_library_metadata",
        "ai_conversations",
        "portable_sections",
        "portable_import_provenance",
        "safety_backup_failure_receipts",
        "scenario_revision_high_water",
        "scenario_identity_owners",
        "ai_messages",
        "schema_migrations",
    ] {
        assert!(
            diagnostics
                .tables
                .iter()
                .any(|candidate| candidate == table)
        );
    }
    for index in [
        "scenarios_by_recency",
        "solve_runs_by_scenario",
        "accepted_solutions_by_scenario",
        "ai_conversations_by_scenario",
        "command_journal_by_history",
    ] {
        assert!(
            diagnostics
                .indexes
                .iter()
                .any(|candidate| candidate == index)
        );
    }
    Ok(())
}

#[tokio::test]
async fn released_migration_checksum_mismatch_is_rejected() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    drop(store);

    let connection = Connection::open(&path)?;
    assert_eq!(
        connection.execute(
            "UPDATE schema_migrations SET checksum = ?1 WHERE version = 1",
            ["checksum-for-changed-released-sql"],
        )?,
        1
    );
    drop(connection);

    assert!(matches!(
        SqliteScenarioStore::open(&path).await,
        Err(StoreError::MigrationChanged { version: 1 })
    ));
    let connection = Connection::open(&path)?;
    let retained_checksum: String = connection.query_row(
        "SELECT checksum FROM schema_migrations WHERE version = 1",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(retained_checksum, "checksum-for-changed-released-sql");
    Ok(())
}

#[tokio::test]
async fn newer_database_is_rejected_without_schema_mutation() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let connection = Connection::open(&path)?;
    connection
        .execute_batch("CREATE TABLE future_marker (value TEXT); PRAGMA user_version = 2;")?;
    drop(connection);
    let bytes_before = std::fs::read(&path)?;
    let wal_path = directory.path().join("library.sqlite3-wal");
    let shm_path = directory.path().join("library.sqlite3-shm");
    assert!(!wal_path.exists());
    assert!(!shm_path.exists());

    let result = SqliteScenarioStore::open(&path).await;
    assert!(matches!(
        result,
        Err(StoreError::NewerSchema {
            found: 2,
            supported: 1
        })
    ));
    assert_eq!(std::fs::read(&path)?, bytes_before);
    assert!(!wal_path.exists());
    assert!(!shm_path.exists());
    let connection = Connection::open(&path)?;
    let marker_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'future_marker')",
        [],
        |row| row.get(0),
    )?;
    let migration_registry_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    assert!(marker_exists);
    assert!(!migration_registry_exists);
    Ok(())
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn migration_failpoint_rolls_back_schema_and_registry() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let result = SqliteScenarioStore::open_with_options(
        &path,
        OpenOptions::new(SnapshotPolicy::default()).with_failpoint(Failpoint::AfterMigrationSql),
    )
    .await;
    assert!(matches!(result, Err(StoreError::InjectedFailure)));

    let connection = Connection::open(&path)?;
    let application_tables: u32 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    let schema_version: u32 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    assert_eq!(application_tables, 0);
    assert_eq!(schema_version, 0);
    Ok(())
}

#[tokio::test]
async fn startup_marks_running_solve_runs_interrupted() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("library.sqlite3");
    let scenario_id = scenario_id(6)?;
    let expected = document(scenario_id)?;
    let (store, _) = SqliteScenarioStore::open(&path).await?;
    store
        .create_project(NewProject {
            document: expected.clone(),
        })
        .await?;
    drop(store);

    let run_id = SolveRunId::from_uuid(Uuid::now_v7());
    let connection = Connection::open(&path)?;
    connection.execute(
        "INSERT INTO solve_runs (id, scenario_id, scenario_revision, input_hash, backend_id, backend_version, status, options_json, started_at) VALUES (?1, ?2, 1, 'hash', 'backend', '1', 'running', '{}', ?3)",
        params![run_id.to_string(), scenario_id.to_string(), CREATED],
    )?;
    drop(connection);

    let (reopened, outcome) = SqliteScenarioStore::open(&path).await?;
    assert_eq!(outcome.recovery.interrupted_solve_run_ids, vec![run_id]);
    let persisted = reopened.get_project(scenario_id).await?;
    assert_eq!(persisted.summary.revision, Revision::INITIAL);
    assert_eq!(persisted.document, expected);

    let connection = Connection::open(&path)?;
    let (status, finished_at, error_json): (String, Option<String>, Option<String>) = connection
        .query_row(
            "SELECT status, finished_at, error_json FROM solve_runs WHERE id = ?1",
            [run_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
    assert_eq!(status, "interrupted");
    assert!(finished_at.is_some());
    assert_eq!(error_json.as_deref(), Some("{\"code\":\"interrupted\"}"));
    drop(reopened);
    Ok(())
}
