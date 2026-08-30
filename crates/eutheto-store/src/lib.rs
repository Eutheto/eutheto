//! `SQLite`-backed scenario persistence.
//!
//! All access to a store is serialized through one dedicated thread which owns
//! the sole `SQLite` connection. Callers never receive a connection or a
//! transaction and therefore cannot accidentally bypass revision checks.

use eutheto_export::{
    ApplicationMetadata, PortableScenario, SemanticCapability, collect_scenario_owned_uuids,
    collect_self_declared_uuids, validate_scenario_owned_uuid_uniqueness,
};
use eutheto_import::{
    AppliedMigration, ImportProvenance, MigrationRegistryKind, PreviewBinding,
    RestoreAuthorization, RestoreMode, SafetyBackupEvidence, StagedBackupRestore,
    StagedDisposition, StagedImport,
};
use eutheto_types::{
    ActorRef, BundleId, CommandId, CommandSource, MAX_SCENARIO_DOCUMENT_BYTES, PackId,
    PortableAsset, PortableJsonLimits, ProjectMetadataDto, ProjectSummaryDto, Revision,
    Rfc3339Timestamp, ScenarioDocument, ScenarioId, ScenarioRevisionReference, SolveRunId,
    SupplementalIdentity, SupplementalSectionKind, extract_result_dependency, extract_result_id,
    extract_scenario_references, validate_nonsecret_portable_json,
};
use rusqlite::{Connection, MAIN_DB, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{File, OpenOptions as FileOpenOptions};
use std::io::{self, Read};
use std::num::NonZeroU32;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::thread;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

const CURRENT_SCHEMA_VERSION: u32 = 1;
const INITIAL_MIGRATION: &str = include_str!("../../../migrations/V1_initial_schema.sql");
const MAX_SNAPSHOT_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
const IMPORT_PROVENANCE_RETENTION_POLICY_VERSION: u32 = 1;
const MAX_IMPORT_PROVENANCE_ROWS: u64 = 128;
const MAX_IMPORT_PROVENANCE_BYTES: u64 = 4 * 1024 * 1024;
const LIBRARY_REVISION_KEY: &str = "portable_library_revision";
const MAX_SAFETY_BACKUP_FAILURE_RECEIPTS: u64 = 16;
const MAX_RECEIPT_PROOF_BYTES: usize = 1_024;
const MAX_RECEIPT_BINDING_BYTES: usize = 4 * 1_024;
const MAX_RECEIPT_SAFE_REASON_BYTES: usize = 4 * 1_024;

/// Errors emitted by the persistence boundary. Database diagnostics are kept
/// as a source for structured logs; callers should map these to user-safe text.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("the database actor is unavailable")]
    ActorUnavailable,
    #[error("failed to start the database actor: {0}")]
    ActorStart(String),
    #[error("database operation failed")]
    Database(#[source] rusqlite::Error),
    #[error("stored JSON is invalid")]
    Json(#[source] serde_json::Error),
    #[error("scenario {0} was not found")]
    ScenarioNotFound(ScenarioId),
    #[error("revision conflict")]
    Conflict {
        expected: Revision,
        actual: Revision,
    },
    #[error(
        "library revision conflict: expected {}, actual {}",
        .expected.value(),
        .actual.value()
    )]
    LibraryConflict {
        expected: Revision,
        actual: Revision,
    },
    #[error("scenario {0} already exists")]
    ScenarioAlreadyExists(ScenarioId),
    #[error("command application failed ({code}): {message}")]
    CommandApplication { code: String, message: String },
    #[error("staged library apply is invalid: {0}")]
    InvalidStagedApply(String),
    #[error("scenario identity graph is invalid: {0}")]
    InvalidScenarioIdentity(String),
    #[error("portable identity is already owned: {0}")]
    IdentityCollision(Uuid),
    #[error("safety-backup failure receipt is invalid: {0}")]
    InvalidSafetyBackupFailureReceipt(String),
    #[error("safety-backup failure receipt is missing or does not match the staged restore")]
    SafetyBackupFailureReceiptRejected,
    #[error("project duplicate identity mapping is invalid: {0}")]
    InvalidDuplicateMapping(String),
    #[error("the database schema version {found} is newer than supported version {supported}")]
    NewerSchema { found: u32, supported: u32 },
    #[error("released migration {version} does not match its embedded checksum")]
    MigrationChanged { version: u32 },
    #[error("database integrity check failed: {0}")]
    Integrity(String),
    #[error("snapshot policy is outside the supported bounds")]
    InvalidSnapshotPolicy,
    #[error("scenario document exceeds the snapshot input limit")]
    SnapshotTooLarge,
    #[error("scenario document exceeds the authoritative size limit")]
    ScenarioDocumentTooLarge,
    #[error("snapshot compression failed")]
    Compression(#[source] io::Error),
    #[error("undo is unavailable")]
    NoUndo,
    #[error("redo is unavailable")]
    NoRedo,
    #[error("the selected command is not reversible")]
    CommandNotReversible,
    #[error("a new command would discard redo history; explicit truncation is required")]
    RedoBranchRequiresTruncation,
    #[error("the backup destination already exists")]
    BackupDestinationExists,
    #[error("numeric database value is outside the supported range")]
    NumericRange,
    #[error("private storage path validation failed")]
    PrivatePath(#[source] io::Error),
    #[cfg(debug_assertions)]
    #[error("injected persistence failure")]
    InjectedFailure,
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Bounded periodic snapshot configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotPolicy {
    interval: NonZeroU32,
    max_document_bytes: u64,
    compression_level: i32,
}

impl SnapshotPolicy {
    /// Creates a policy. Intervals are limited to 1..=10,000 commands,
    /// documents to 16 MiB..=64 MiB, and zstd levels to 1..=19. The lower
    /// document bound guarantees every legal authoritative document can be
    /// snapshotted when its interval is reached.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::InvalidSnapshotPolicy`] when any bound is violated.
    pub fn new(
        interval: NonZeroU32,
        max_document_bytes: u64,
        compression_level: i32,
    ) -> Result<Self, StoreError> {
        if interval.get() > 10_000
            || !(MAX_SCENARIO_DOCUMENT_BYTES..=MAX_SNAPSHOT_DOCUMENT_BYTES)
                .contains(&max_document_bytes)
            || !(1..=19).contains(&compression_level)
        {
            return Err(StoreError::InvalidSnapshotPolicy);
        }
        Ok(Self {
            interval,
            max_document_bytes,
            compression_level,
        })
    }

    /// Number of committed commands between snapshots.
    #[must_use]
    pub const fn interval(self) -> NonZeroU32 {
        self.interval
    }

    /// Maximum uncompressed scenario document accepted by the compressor.
    #[must_use]
    pub const fn max_document_bytes(self) -> u64 {
        self.max_document_bytes
    }
}

impl Default for SnapshotPolicy {
    fn default() -> Self {
        Self {
            interval: NonZeroU32::MIN.saturating_add(49),
            max_document_bytes: MAX_SCENARIO_DOCUMENT_BYTES,
            compression_level: 3,
        }
    }
}

/// Configuration applied before the actor starts accepting work.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenOptions {
    pub snapshot_policy: SnapshotPolicy,
    #[cfg(debug_assertions)]
    pub failpoint: Option<Failpoint>,
}

/// One-shot test failure locations. This API is absent from optimized builds.
#[cfg(debug_assertions)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failpoint {
    AfterMigrationSql,
    AfterDocumentWrite,
    AfterSupplementalWrite,
}
impl OpenOptions {
    /// Uses a validated snapshot policy with no injected failure.
    #[must_use]
    pub const fn new(snapshot_policy: SnapshotPolicy) -> Self {
        Self {
            snapshot_policy,
            #[cfg(debug_assertions)]
            failpoint: None,
        }
    }

    /// Arms an initialization failpoint in debug/test builds.
    #[cfg(debug_assertions)]
    #[must_use]
    pub const fn with_failpoint(mut self, failpoint: Failpoint) -> Self {
        self.failpoint = Some(failpoint);
        self
    }
}

/// Result of startup recovery work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializationOutcome {
    pub schema_version: u32,
    pub applied_migrations: Vec<u32>,
    pub integrity: IntegrityOutcome,
    pub recovery: RecoveryOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrityOutcome {
    Ok,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryOutcome {
    pub interrupted_solve_run_ids: Vec<SolveRunId>,
}

/// Authoritative document used to create one persisted scenario.
#[derive(Clone, Debug)]
pub struct NewProject {
    pub document: ScenarioDocument,
}

/// A typed persisted project row, intentionally narrower than the authoritative document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: ScenarioId,
    pub domain_pack_id: PackId,
    pub domain_schema_version: u32,
    pub title: String,
    pub description: Option<String>,
    pub revision: Revision,
    pub created_at: Rfc3339Timestamp,
    pub updated_at: Rfc3339Timestamp,
    pub last_opened_at: Option<Rfc3339Timestamp>,
    pub archived_at: Option<Rfc3339Timestamp>,
}

impl From<&ProjectSummary> for ProjectSummaryDto {
    fn from(summary: &ProjectSummary) -> Self {
        Self {
            scenario_id: summary.id,
            title: summary.title.clone(),
            domain_pack_id: summary.domain_pack_id.clone(),
            revision: summary.revision,
            updated_at: summary.updated_at,
            archived: summary.archived_at.is_some(),
        }
    }
}

impl From<&ProjectSummary> for ProjectMetadataDto {
    fn from(summary: &ProjectSummary) -> Self {
        Self {
            scenario_id: summary.id,
            title: summary.title.clone(),
            description: summary.description.clone().unwrap_or_default(),
            domain_pack: eutheto_types::DomainPackRef {
                id: summary.domain_pack_id.clone(),
                schema_version: summary.domain_schema_version,
            },
            revision: summary.revision,
            created_at: summary.created_at,
            updated_at: summary.updated_at,
            archived_at: summary.archived_at,
        }
    }
}
/// Portable envelope values that are not part of the authoritative document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PortableWrapperMetadata {
    pub required_capabilities: BTreeSet<SemanticCapability>,
    pub semantic_extensions: BTreeMap<String, Value>,
    pub extensions: BTreeMap<String, Value>,
}

/// Complete stored scenario view.
#[derive(Clone, Debug)]
pub struct StoredProject {
    pub summary: ProjectSummary,
    pub document: ScenarioDocument,
    pub portable: PortableWrapperMetadata,
}

/// Exact immutable portable scenario revision required by a retained result.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredScenarioRevision {
    pub scenario: PortableScenario,
}

/// Supplemental portable JSON bytes and inert assets with their exact declarations.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PortableSupplementalSections {
    pub results: BTreeMap<String, Vec<u8>>,
    pub shared_records: BTreeMap<String, Vec<u8>>,
    pub preferences: BTreeMap<String, Vec<u8>>,
    pub assets: BTreeMap<String, PortableAsset>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedMigrationRegistry {
    Outer,
    Portable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAppliedMigration {
    pub registry: PersistedMigrationRegistry,
    pub name: String,
    pub from_version: u32,
    pub to_version: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedDisposition {
    Create,
    CreateCopy,
    Replace,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedPreviewBinding {
    pub file_sha256: String,
    pub options_sha256: String,
    pub local_library_revision: Revision,
    pub format_version: u32,
    pub schema_version: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioImportSource {
    pub scenario_id: ScenarioId,
    pub original_scenario_id: ScenarioId,
    pub source_revision: Revision,
    pub disposition: PersistedDisposition,
    pub id_remap: BTreeMap<Uuid, Uuid>,
}

/// Provenance of one committed portable apply, including the source revision
/// even when the local replacement revision was advanced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedImportProvenance {
    pub source_bundle_id: BundleId,
    pub source_application: ApplicationMetadata,
    pub original_format_version: u32,
    pub original_schema_version: u32,
    pub source_file_sha256: String,
    pub applied_migrations: Vec<PersistedAppliedMigration>,
    pub scenario_sources: Vec<ScenarioImportSource>,
    pub binding: PersistedPreviewBinding,
    pub source_created_at: Rfc3339Timestamp,
    pub applied_at: Rfc3339Timestamp,
}

/// Transactionally consistent view used by export and import preview.
#[derive(Clone, Debug)]
pub struct LibrarySnapshot {
    pub revision: Revision,
    pub projects: Vec<StoredProject>,
    pub settings: BTreeMap<String, AppSetting<Value>>,
    pub scenario_revisions: Vec<StoredScenarioRevision>,
    pub scenario_revision_high_water: BTreeMap<ScenarioId, Revision>,
    pub scenario_identity_owners: BTreeMap<Uuid, ScenarioId>,
    pub manifest_extensions: BTreeMap<String, Value>,
    pub nonsemantic_extensions: BTreeSet<String>,
    pub sections: PortableSupplementalSections,
    pub provenance: Vec<PersistedImportProvenance>,
    pub supplemental_identities: BTreeSet<SupplementalIdentity>,
    pub supplemental_identity_owners: BTreeMap<Uuid, SupplementalIdentity>,
}

/// Minimal transactionally consistent library identity used by support flows
/// that must not deserialize user-authored content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LibraryMetadataSnapshot {
    pub revision: Revision,
    pub scenario_count: u64,
}

/// One owner-private authorization receipt recorded after a real safety-backup failure.
#[derive(Clone, Debug)]
pub struct SafetyBackupFailureReceipt {
    pub proof: String,
    pub binding: PreviewBinding,
    pub collision_plan_sha256: String,
    pub safe_reason: String,
    pub created_at: Rfc3339Timestamp,
}
/// Fully staged portable mutation accepted by one store transaction.
#[derive(Clone, Debug)]
pub enum StagedLibraryApply {
    Import(StagedImport),
    BackupRestore {
        restore: StagedBackupRestore,
        settings: BTreeMap<String, AppSetting<Value>>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LibraryApplyOutcome {
    pub library_revision: Revision,
    pub created: usize,
    pub replaced: usize,
    pub removed: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectListScope {
    Active,
    Archived,
    All,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedoBranchPolicy {
    Reject,
    Truncate,
}

/// Typed audit material supplied by the pure command layer.
#[derive(Clone, Debug)]
pub struct JournalWrite {
    pub command_type: String,
    pub command: Value,
    pub command_id: CommandId,
    pub inverse: Option<Value>,
    pub actor: ActorRef,
    pub source: CommandSource,
    pub summary: String,
    pub created_at: Rfc3339Timestamp,
}

/// The value produced by a pure command callback and committed atomically.
#[derive(Clone, Debug)]
pub struct CommandWrite<T> {
    pub document: ScenarioDocument,
    pub journal: JournalWrite,
    pub output: T,
}

/// Result returned after the transaction commits.
#[derive(Clone, Debug)]
pub struct CommittedCommand<T> {
    pub new_revision: Revision,
    pub output: T,
}

/// A persisted history row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryEntry {
    pub id: CommandId,
    pub revision_before: Revision,
    pub revision_after: Revision,
    pub command_type: String,
    pub command: Value,
    pub inverse: Option<Value>,
    pub actor: ActorRef,
    pub source: CommandSource,
    pub summary: String,
    pub created_at: Rfc3339Timestamp,
    pub history_sequence: u64,
    pub branch_generation: u64,
    pub applied: bool,
}
/// Live connection settings and installed schema objects, used by startup
/// diagnostics without exposing `SQLite` access to callers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreDiagnostics {
    pub foreign_keys: bool,
    pub journal_mode: String,
    pub synchronous: u32,
    pub busy_timeout_ms: u32,
    pub trusted_schema: bool,
    pub schema_version: u32,
    pub tables: Vec<String>,
    pub indexes: Vec<String>,
}

/// Input supplied to undo and redo callbacks.
#[derive(Clone, Debug)]
pub struct HistoryCommand {
    pub document: ScenarioDocument,
    pub command: Value,
    pub entry: HistoryEntry,
    pub target_document_updated_at: Rfc3339Timestamp,
}

/// App setting with its last update timestamp.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppSetting<T> {
    pub value: T,
    pub updated_at: Rfc3339Timestamp,
}

type ActorOperation = Box<dyn FnOnce(&mut Connection) + Send + 'static>;

struct StoreActor {
    sender: Option<mpsc::UnboundedSender<ActorOperation>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for StoreActor {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(handle) = self.thread.take()
            && handle.thread().id() != thread::current().id()
        {
            let _ignored = handle.join();
        }
    }
}

/// Async handle to a dedicated-thread `SQLite` actor. The handle is cheap to
/// clone and is suitable for sharing through [`Arc`].
#[derive(Clone)]
pub struct SqliteScenarioStore {
    actor: Arc<StoreActor>,
    snapshot_policy: SnapshotPolicy,
    #[cfg(debug_assertions)]
    failpoint: Arc<std::sync::Mutex<Option<Failpoint>>>,
}

impl fmt::Debug for SqliteScenarioStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteScenarioStore")
            .field("snapshot_policy", &self.snapshot_policy)
            .finish_non_exhaustive()
    }
}

impl SqliteScenarioStore {
    /// Opens or creates a database and returns both the actor and startup
    /// recovery outcome.
    ///
    /// # Errors
    ///
    /// Returns an error if the actor thread cannot start or report its
    /// initialization result, if the database cannot be opened or initialized,
    /// or if its schema, migration checksum, or integrity check is invalid. In
    /// debug builds, an armed initialization failpoint also returns an error.
    pub async fn open(path: impl AsRef<Path>) -> Result<(Self, InitializationOutcome), StoreError> {
        Self::open_with_options(path, OpenOptions::default()).await
    }

    /// Opens or creates a database with explicit actor and snapshot options.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid snapshot policy, actor startup or
    /// availability failure, database open or initialization failure, an
    /// unsupported or invalid schema, a changed migration, or failed integrity
    /// checks. In debug builds, an armed initialization failpoint also returns
    /// an error.
    pub async fn open_with_options(
        path: impl AsRef<Path>,
        options: OpenOptions,
    ) -> Result<(Self, InitializationOutcome), StoreError> {
        // Re-run validation even though fields are private so a future policy
        // expansion cannot accidentally bypass the bounds.
        let policy = SnapshotPolicy::new(
            options.snapshot_policy.interval,
            options.snapshot_policy.max_document_bytes,
            options.snapshot_policy.compression_level,
        )?;
        let path = path.as_ref().to_path_buf();
        let (sender, mut receiver) = mpsc::unbounded_channel::<ActorOperation>();
        let (initialization_sender, initialization_receiver) = oneshot::channel();
        #[cfg(debug_assertions)]
        let failpoint = Arc::new(std::sync::Mutex::new(options.failpoint));
        #[cfg(debug_assertions)]
        let actor_failpoint = Arc::clone(&failpoint);

        let actor_thread = thread::Builder::new()
            .name("eutheto-sqlite-store".to_owned())
            .spawn(move || {
                let result = prepare_private_database_path(&path).and_then(|guard| {
                    preflight_schema_version(&guard.file)
                        .and_then(|()| open_connection(&path))
                        .and_then(|mut connection| {
                            verify_private_database_path(&path, &guard)?;
                            initialize_connection(
                                &mut connection,
                                #[cfg(debug_assertions)]
                                &actor_failpoint,
                            )
                            .map(|outcome| (connection, outcome))
                        })
                });
                match result {
                    Ok((mut connection, outcome)) => {
                        if initialization_sender.send(Ok(outcome)).is_err() {
                            return;
                        }
                        while let Some(operation) = receiver.blocking_recv() {
                            operation(&mut connection);
                        }
                    }
                    Err(error) => {
                        let _ignored = initialization_sender.send(Err(error));
                    }
                }
            })
            .map_err(|error| StoreError::ActorStart(error.to_string()))?;

        let outcome = initialization_receiver
            .await
            .map_err(|_| StoreError::ActorUnavailable)??;
        Ok((
            Self {
                actor: Arc::new(StoreActor {
                    sender: Some(sender),
                    thread: Some(actor_thread),
                }),
                snapshot_policy: policy,
                #[cfg(debug_assertions)]
                failpoint,
            },
            outcome,
        ))
    }

    async fn call<T, F>(&self, operation: F) -> Result<T, StoreError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, StoreError> + Send + 'static,
    {
        let (result_sender, result_receiver) = oneshot::channel();
        self.actor
            .sender
            .as_ref()
            .ok_or(StoreError::ActorUnavailable)?
            .send(Box::new(move |connection| {
                let result = operation(connection);
                let _ignored = result_sender.send(result);
            }))
            .map_err(|_| StoreError::ActorUnavailable)?;
        result_receiver
            .await
            .map_err(|_| StoreError::ActorUnavailable)?
    }

    /// Creates a project at revision zero.
    ///
    /// # Errors
    ///
    /// Returns an error if the scenario already exists, the document cannot be
    /// serialized, a numeric value is out of range, the storage transaction
    /// fails, or the database actor is unavailable.
    pub async fn create_project(&self, project: NewProject) -> Result<StoredProject, StoreError> {
        self.call(move |connection| {
            let scenario_id = project.document.scenario_id;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            ensure_scenario_id_unseen(&transaction, scenario_id)?;
            insert_project(
                &transaction,
                &project.document,
                Revision::INITIAL,
                None,
                &PortableWrapperMetadata::default(),
            )?;
            validate_global_identity_ownership(&transaction)?;
            increment_library_revision(&transaction)?;
            transaction.commit()?;
            load_project(connection, scenario_id)
        })
        .await
    }

    /// Lists project summaries in the requested archive scope.
    ///
    /// # Errors
    ///
    /// Returns an error if the database actor or query fails, or if persisted
    /// identifiers, timestamps, or numeric values are invalid.
    pub async fn list_projects(
        &self,
        scope: ProjectListScope,
    ) -> Result<Vec<ProjectSummary>, StoreError> {
        self.call(move |connection| {
            let predicate = match scope {
                ProjectListScope::Active => "archived_at IS NULL",
                ProjectListScope::Archived => "archived_at IS NOT NULL",
                ProjectListScope::All => "1 = 1",
            };
            let sql = format!("SELECT id, domain_pack_id, domain_schema_version, title, description, revision, created_at, updated_at, last_opened_at, archived_at FROM scenarios WHERE {predicate} ORDER BY updated_at DESC, id");
            let mut statement = connection.prepare(&sql)?;
            let mut rows = statement.query([])?;
            let mut summaries = Vec::new();
            while let Some(row) = rows.next()? {
                summaries.push(read_summary_row(row)?);
            }
            Ok(summaries)
        })
        .await
    }

    /// Reads only the portable-library revision and scenario count from one
    /// transaction, without loading user-authored documents or settings.
    ///
    /// # Errors
    ///
    /// Returns an error if the actor or snapshot transaction fails, or if the
    /// persisted revision or count is outside supported numeric bounds.
    pub async fn library_metadata_snapshot(&self) -> Result<LibraryMetadataSnapshot, StoreError> {
        self.call(move |connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let revision = library_revision(&transaction)?;
            let scenario_count: i64 =
                transaction.query_row("SELECT COUNT(*) FROM scenarios", [], |row| row.get(0))?;
            let scenario_count = i64_to_u64(scenario_count)?;
            transaction.commit()?;
            Ok(LibraryMetadataSnapshot {
                revision,
                scenario_count,
            })
        })
        .await
    }

    /// Holds a cross-process `SQLite` write lease from revision validation through
    /// one caller-supplied portable publication commit point.
    ///
    /// The callback should perform only the already-prepared atomic filesystem
    /// commit. Expensive encoding, temporary-file writing, and verification
    /// belong before this lease.
    ///
    /// # Errors
    ///
    /// Returns a revision conflict, missing scenario, database, or actor error.
    /// Callback failures are returned as the inner result after the lease is
    /// released.
    pub async fn with_publication_revision_lease<T, E, F>(
        &self,
        expected_library_revision: Revision,
        expected_scenario: Option<(ScenarioId, Revision)>,
        publish: F,
    ) -> Result<Result<T, E>, StoreError>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce() -> Result<T, E> + Send + 'static,
    {
        self.call(move |connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let actual_library_revision = library_revision(&transaction)?;
            if actual_library_revision != expected_library_revision {
                return Err(StoreError::LibraryConflict {
                    expected: expected_library_revision,
                    actual: actual_library_revision,
                });
            }
            if let Some((scenario_id, expected_revision)) = expected_scenario {
                let actual_revision = scenario_revision(&transaction, scenario_id)?;
                ensure_revision(expected_revision, actual_revision.value())?;
            }
            let publication = publish();
            // This transaction is a read-only writer lease. Dropping it rolls
            // back no state and cannot obscure a completed filesystem commit.
            drop(transaction);
            Ok(publication)
        })
        .await
    }

    /// Reads the portable-library revision, authoritative documents and wrapper
    /// metadata, application settings, supplemental identities, JSON records,
    /// assets and their declarations, and import provenance from one snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the actor or snapshot transaction fails, or if any
    /// stored identifier, timestamp, numeric value, or JSON payload is invalid.
    pub async fn library_snapshot(&self) -> Result<LibrarySnapshot, StoreError> {
        self.call(move |connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let revision = library_revision(&transaction)?;
            let mut statement = transaction.prepare("SELECT id FROM scenarios ORDER BY id")?;
            let ids = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            let mut projects = Vec::with_capacity(ids.len());
            for id in ids {
                let scenario_id = id.parse().map_err(|error| {
                    StoreError::Integrity(format!("invalid stored scenario id: {error}"))
                })?;
                projects.push(load_project(&transaction, scenario_id)?);
            }
            let scenario_revisions = load_retained_scenario_revisions(&transaction)?;
            let scenario_revision_high_water = load_scenario_revision_high_water(&transaction)?;
            let scenario_identity_owners = load_scenario_identity_owners(&transaction)?;
            let settings = load_all_settings(&transaction)?;
            let (manifest_extensions, nonsemantic_extensions) =
                load_portable_library_metadata(&transaction)?;
            let supplemental_identity_owners = load_supplemental_identity_owners(&transaction)?;
            let sections = load_portable_sections(&transaction)?;
            let supplemental_identities = portable_section_identities(&sections);
            let provenance = load_import_provenance(&transaction)?;
            transaction.commit()?;
            Ok(LibrarySnapshot {
                revision,
                projects,
                settings,
                scenario_revisions,
                scenario_revision_high_water,
                scenario_identity_owners,
                manifest_extensions,
                nonsemantic_extensions,
                sections,
                provenance,
                supplemental_identities,
                supplemental_identity_owners,
            })
        })
        .await
    }

    /// Applies a fully staged portable import or backup restore atomically.
    ///
    /// # Errors
    ///
    /// Returns an error when the staged input is invalid or stale, a referenced
    /// scenario is missing or already occupied, a document cannot be
    /// serialized, a numeric value is out of range, the storage transaction
    /// fails, or the database actor is unavailable.
    pub async fn apply_staged_library(
        &self,
        staged: StagedLibraryApply,
        applied_at: Rfc3339Timestamp,
    ) -> Result<LibraryApplyOutcome, StoreError> {
        #[cfg(debug_assertions)]
        let failpoint = Arc::clone(&self.failpoint);
        self.call(move |connection| {
            apply_staged_library_transaction(
                connection,
                staged,
                applied_at,
                #[cfg(debug_assertions)]
                &failpoint,
            )
        })
        .await
    }

    /// Records one authorization receipt after a real safety-backup attempt fails.
    ///
    /// The plaintext proof is hashed before the database actor receives the
    /// operation and is never persisted.
    ///
    /// # Errors
    ///
    /// Returns an error when receipt fields violate their bounds, the proof was
    /// already recorded, or the database actor or transaction fails.
    pub async fn record_safety_backup_failure_receipt(
        &self,
        receipt: SafetyBackupFailureReceipt,
    ) -> Result<(), StoreError> {
        let SafetyBackupFailureReceipt {
            proof,
            binding,
            collision_plan_sha256,
            safe_reason,
            created_at,
        } = receipt;
        validate_failure_receipt_fields(&proof, &collision_plan_sha256, &safe_reason)?;
        let proof_sha256 = eutheto_export::sha256_hex(proof.as_bytes());
        let binding_json = eutheto_export::canonical_json(&binding)
            .map_err(|error| StoreError::InvalidSafetyBackupFailureReceipt(error.to_string()))?;
        if binding_json.len() > MAX_RECEIPT_BINDING_BYTES {
            return Err(StoreError::InvalidSafetyBackupFailureReceipt(
                "preview binding exceeds the receipt size limit".to_owned(),
            ));
        }
        if binding_json
            .windows(proof.len())
            .any(|window| window == proof.as_bytes())
        {
            return Err(StoreError::InvalidSafetyBackupFailureReceipt(
                "preview binding contains the plaintext proof".to_owned(),
            ));
        }
        self.call(move |connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM safety_backup_failure_receipts WHERE proof_sha256 = ?1)",
                [&proof_sha256],
                |row| row.get(0),
            )?;
            if exists {
                return Err(StoreError::InvalidSafetyBackupFailureReceipt(
                    "proof was already recorded".to_owned(),
                ));
            }
            transaction.execute(
                "INSERT INTO safety_backup_failure_receipts (proof_sha256, binding_json, collision_plan_sha256, safe_reason, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    proof_sha256,
                    binding_json,
                    collision_plan_sha256,
                    safe_reason,
                    created_at.to_string(),
                ],
            )?;
            transaction.execute(
                "DELETE FROM safety_backup_failure_receipts WHERE id NOT IN (SELECT id FROM safety_backup_failure_receipts ORDER BY id DESC LIMIT ?1)",
                [u64_to_i64(MAX_SAFETY_BACKUP_FAILURE_RECEIPTS)?],
            )?;
            transaction.commit()?;
            Ok(())
        })
        .await
    }

    /// Returns the authoritative stored project.
    ///
    /// # Errors
    ///
    /// Returns an error if the scenario does not exist, the database actor or
    /// query fails, or its persisted metadata or document JSON is invalid.
    pub async fn get_project(&self, scenario_id: ScenarioId) -> Result<StoredProject, StoreError> {
        self.call(move |connection| load_project(connection, scenario_id))
            .await
    }

    /// Records that a project was opened and returns its authoritative view.
    ///
    /// # Errors
    ///
    /// Returns an error if the scenario does not exist, the database actor or
    /// transaction fails, the library revision overflows, or the authoritative
    /// project contains invalid persisted data.
    pub async fn open_project(
        &self,
        scenario_id: ScenarioId,
        opened_at: Rfc3339Timestamp,
    ) -> Result<StoredProject, StoreError> {
        self.call(move |connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let changed = transaction.execute(
                "UPDATE scenarios SET last_opened_at = ?2 WHERE id = ?1",
                params![scenario_id.to_string(), opened_at.to_string()],
            )?;
            if changed == 0 {
                return Err(StoreError::ScenarioNotFound(scenario_id));
            }
            increment_library_revision(&transaction)?;
            transaction.commit()?;
            load_project(connection, scenario_id)
        })
        .await
    }

    /// Marks a project as archived.
    ///
    /// # Errors
    ///
    /// Returns an error if the scenario does not exist, its revision differs
    /// from `expected_revision`, the transaction fails, or the database actor
    /// is unavailable.
    pub async fn archive_project(
        &self,
        scenario_id: ScenarioId,
        expected_revision: Revision,
        archived_at: Rfc3339Timestamp,
    ) -> Result<(), StoreError> {
        update_archive(self, scenario_id, expected_revision, Some(archived_at)).await
    }

    /// Marks an archived project as active.
    ///
    /// # Errors
    ///
    /// Returns an error if the scenario does not exist, its revision differs
    /// from `expected_revision`, the transaction fails, or the database actor
    /// is unavailable.
    pub async fn unarchive_project(
        &self,
        scenario_id: ScenarioId,
        expected_revision: Revision,
    ) -> Result<(), StoreError> {
        update_archive(self, scenario_id, expected_revision, None).await
    }

    /// Permanently deletes a project, its dependent rows, and supplemental JSON
    /// records that declare a reference to it. Shared inert assets are retained.
    ///
    /// # Errors
    ///
    /// Returns an error if the scenario does not exist, its revision differs
    /// from `expected_revision`, the transaction fails, or the database actor
    /// is unavailable.
    pub async fn delete_project(
        &self,
        scenario_id: ScenarioId,
        expected_revision: Revision,
    ) -> Result<(), StoreError> {
        self.call(move |connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let actual_revision = scenario_revision(&transaction, scenario_id)?;
            ensure_revision(expected_revision, actual_revision.value())?;
            record_scenario_revision_high_water(&transaction, scenario_id, actual_revision)?;
            delete_scenario_referencing_supplemental(&transaction, scenario_id)?;
            transaction.execute(
                "DELETE FROM scenarios WHERE id = ?1",
                [scenario_id.to_string()],
            )?;
            synchronize_retained_scenario_revisions(&transaction, Vec::new())?;
            increment_library_revision(&transaction)?;
            transaction.commit()?;
            Ok(())
        })
        .await
    }

    /// Duplicates only the project document. Journal, snapshots, runs, and
    /// solutions deliberately remain attached to the source project.
    ///
    /// # Errors
    ///
    /// Returns an error if the source scenario is missing, its revision differs
    /// from `expected_source_revision`, the destination already exists, the
    /// source document is malformed or cannot be serialized, a numeric value is
    /// out of range, the transaction fails, or the database actor is
    /// unavailable.
    pub async fn duplicate_project(
        &self,
        source_id: ScenarioId,
        expected_source_revision: Revision,
        new_id: ScenarioId,
        id_remap: BTreeMap<Uuid, Uuid>,
        title: String,
        created_at: Rfc3339Timestamp,
    ) -> Result<StoredProject, StoreError> {
        self.call(move |connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let source = load_project(&transaction, source_id)?;
            ensure_revision(expected_source_revision, source.summary.revision.value())?;
            let occupied = authoritative_occupied_uuids(&transaction)?;
            let (document, portable) = duplicate_project_contents(
                source, new_id, &id_remap, &occupied, &title, created_at,
            )?;
            insert_project(&transaction, &document, Revision::INITIAL, None, &portable)?;
            validate_global_identity_ownership(&transaction)?;
            increment_library_revision(&transaction)?;
            transaction.commit()?;
            load_project(connection, new_id)
        })
        .await
    }

    /// Runs a pure command callback after checking the current revision and
    /// commits the document, journal, cursor, revision, and optional snapshot
    /// in one immediate transaction.
    ///
    /// # Errors
    ///
    /// Returns an error if the scenario is missing, its revision conflicts,
    /// redo history requires explicit truncation, the callback returns a typed
    /// command error, command data cannot be serialized, numeric bounds are
    /// exceeded, snapshot sizing or compression fails, the transaction or actor
    /// fails, or a debug failpoint is triggered.
    pub async fn execute_command<T, F>(
        &self,
        scenario_id: ScenarioId,
        expected_revision: Revision,
        branch_policy: RedoBranchPolicy,
        apply: F,
    ) -> Result<CommittedCommand<T>, StoreError>
    where
        T: Send + 'static,
        F: FnOnce(&ScenarioDocument) -> Result<CommandWrite<T>, StoreError> + Send + 'static,
    {
        let snapshot_policy = self.snapshot_policy;
        #[cfg(debug_assertions)]
        let failpoint = Arc::clone(&self.failpoint);
        self.call(move |connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let project = load_project(&transaction, scenario_id)?;
            let actual_revision = project.summary.revision.value();
            let document = project.document;
            let portable = project.portable;
            ensure_revision(expected_revision, actual_revision)?;
            let (cursor, mut generation, max_sequence) = history_state(&transaction, scenario_id)?;
            if cursor < max_sequence {
                match branch_policy {
                    RedoBranchPolicy::Reject => return Err(StoreError::RedoBranchRequiresTruncation),
                    RedoBranchPolicy::Truncate => {
                        transaction.execute(
                            "DELETE FROM command_journal WHERE scenario_id = ?1 AND history_sequence > ?2",
                            params![scenario_id.to_string(), u64_to_i64(cursor)?],
                        )?;
                        generation = generation.checked_add(1).ok_or(StoreError::NumericRange)?;
                    }
                }
            }
            let write = apply(&document)?;
            let new_revision = checked_revision(actual_revision)?
                .checked_next()
                .map_err(|_| StoreError::NumericRange)?;
            validate_project_owned_uuid_uniqueness(&write.document, new_revision, &portable)
                .map_err(StoreError::InvalidScenarioIdentity)?;
            record_scenario_identity_owners(
                &transaction,
                scenario_id,
                &collect_project_owned_uuids(
                    &write.document,
                    &portable.semantic_extensions,
                    &portable.extensions,
                ),
            )?;
            let sequence = cursor.checked_add(1).ok_or(StoreError::NumericRange)?;
            let document_json = serialize_document(&write.document)?;
            let projection = document_projection(&write.document);
            let command_json = serde_json::to_string(&write.journal.command)?;
            let inverse_json = write
                .journal
                .inverse
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;
            let actor_json = serde_json::to_string(&write.journal.actor)?;
            transaction.execute(
                "UPDATE scenarios SET domain_pack_id = ?2, domain_schema_version = ?3, title = ?4, description = ?5, document_json = ?6, revision = ?7, updated_at = ?8 WHERE id = ?1",
                params![
                    scenario_id.to_string(),
                    projection.domain_pack_id,
                    u32_to_i64(projection.domain_schema_version),
                    projection.title,
                    projection.description,
                    document_json,
                    u64_to_i64(new_revision.value())?,
                    projection.updated_at,
                ],
            )?;
            record_scenario_revision_high_water(
                &transaction,
                scenario_id,
                new_revision,
            )?;
            #[cfg(debug_assertions)]
            consume_failpoint(&failpoint, Failpoint::AfterDocumentWrite)?;
            transaction.execute(
                "INSERT INTO command_journal (id, scenario_id, revision_before, revision_after, command_type, command_json, inverse_json, actor_json, source, summary, created_at, history_sequence, branch_generation) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![write.journal.command_id.to_string(), scenario_id.to_string(), u64_to_i64(actual_revision)?, u64_to_i64(new_revision.value())?, write.journal.command_type, command_json, inverse_json, actor_json, command_source_name(write.journal.source), write.journal.summary, write.journal.created_at.to_string(), u64_to_i64(sequence)?, u64_to_i64(generation)?],
            )?;
            transaction.execute(
                "UPDATE scenario_history_state SET cursor_sequence = ?2, branch_generation = ?3 WHERE scenario_id = ?1",
                params![scenario_id.to_string(), u64_to_i64(sequence)?, u64_to_i64(generation)?],
            )?;
            maybe_snapshot(&transaction, scenario_id, new_revision.value(), sequence, &write.document, &write.journal.created_at, snapshot_policy)?;
            validate_global_identity_ownership(&transaction)?;
            increment_library_revision(&transaction)?;
            transaction.commit()?;
            Ok(CommittedCommand {
                new_revision,
                output: write.output,
            })
        })
        .await
    }

    /// Applies the inverse of the current history entry.
    ///
    /// # Errors
    ///
    /// Returns an error if the scenario is missing, its revision conflicts,
    /// undo is unavailable, the command is not reversible, the callback returns
    /// a typed command error, stored history or JSON is invalid, numeric bounds
    /// are exceeded, or the transaction or actor fails.
    pub async fn undo<T, F>(
        &self,
        scenario_id: ScenarioId,
        expected_revision: Revision,
        applied_at: Rfc3339Timestamp,
        apply_inverse: F,
    ) -> Result<CommittedCommand<T>, StoreError>
    where
        T: Send + 'static,
        F: FnOnce(HistoryCommand) -> Result<(ScenarioDocument, T), StoreError> + Send + 'static,
    {
        self.move_history(
            scenario_id,
            expected_revision,
            applied_at,
            HistoryDirection::Undo,
            apply_inverse,
        )
        .await
    }

    /// Reapplies the next command in the redo history.
    ///
    /// # Errors
    ///
    /// Returns an error if the scenario is missing, its revision conflicts,
    /// redo is unavailable, the callback returns a typed command error, stored
    /// history or JSON is invalid, numeric bounds are exceeded, or the
    /// transaction or actor fails.
    pub async fn redo<T, F>(
        &self,
        scenario_id: ScenarioId,
        expected_revision: Revision,
        applied_at: Rfc3339Timestamp,
        apply_command: F,
    ) -> Result<CommittedCommand<T>, StoreError>
    where
        T: Send + 'static,
        F: FnOnce(HistoryCommand) -> Result<(ScenarioDocument, T), StoreError> + Send + 'static,
    {
        self.move_history(
            scenario_id,
            expected_revision,
            applied_at,
            HistoryDirection::Redo,
            apply_command,
        )
        .await
    }

    async fn move_history<T, F>(
        &self,
        scenario_id: ScenarioId,
        expected_revision: Revision,
        applied_at: Rfc3339Timestamp,
        direction: HistoryDirection,
        apply: F,
    ) -> Result<CommittedCommand<T>, StoreError>
    where
        T: Send + 'static,
        F: FnOnce(HistoryCommand) -> Result<(ScenarioDocument, T), StoreError> + Send + 'static,
    {
        self.call(move |connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let project = load_project(&transaction, scenario_id)?;
            let actual_revision = project.summary.revision.value();
            let document = project.document;
            let portable = project.portable;
            ensure_revision(expected_revision, actual_revision)?;
            let (cursor, _generation, max_sequence) = history_state(&transaction, scenario_id)?;
            let target_sequence = match direction {
                HistoryDirection::Undo if cursor == 0 => return Err(StoreError::NoUndo),
                HistoryDirection::Undo => cursor,
                HistoryDirection::Redo if cursor >= max_sequence => return Err(StoreError::NoRedo),
                HistoryDirection::Redo => cursor.checked_add(1).ok_or(StoreError::NumericRange)?,
            };
            let entry = load_history_entry(&transaction, scenario_id, target_sequence, cursor)?;
            let command = match direction {
                HistoryDirection::Undo => entry.inverse.clone().ok_or(StoreError::CommandNotReversible)?,
                HistoryDirection::Redo => entry.command.clone(),
            };
            let new_cursor = match direction {
                HistoryDirection::Undo => cursor.checked_sub(1).ok_or(StoreError::NumericRange)?,
                HistoryDirection::Redo => target_sequence,
            };
            let target_document_updated_at = match direction {
                HistoryDirection::Undo if new_cursor == 0 => document.metadata.created_at,
                HistoryDirection::Undo => {
                    load_history_entry(&transaction, scenario_id, new_cursor, cursor)?.created_at
                }
                HistoryDirection::Redo => entry.created_at,
            };
            let (updated_document, output) = apply(HistoryCommand {
                document,
                command,
                entry,
                target_document_updated_at,
            })?;
            let new_revision = checked_revision(actual_revision)?
                .checked_next()
                .map_err(|_| StoreError::NumericRange)?;
            validate_project_owned_uuid_uniqueness(&updated_document, new_revision, &portable)
                .map_err(StoreError::InvalidScenarioIdentity)?;
            record_scenario_identity_owners(
                &transaction,
                scenario_id,
                &collect_project_owned_uuids(
                    &updated_document,
                    &portable.semantic_extensions,
                    &portable.extensions,
                ),
            )?;
            let projection = document_projection(&updated_document);
            transaction.execute(
                "UPDATE scenarios SET domain_pack_id = ?2, domain_schema_version = ?3, title = ?4, description = ?5, document_json = ?6, revision = ?7, updated_at = ?8 WHERE id = ?1",
                params![
                    scenario_id.to_string(),
                    projection.domain_pack_id,
                    u32_to_i64(projection.domain_schema_version),
                    projection.title,
                    projection.description,
                    serialize_document(&updated_document)?,
                    u64_to_i64(new_revision.value())?,
                    applied_at.to_string(),
                ],
            )?;
            record_scenario_revision_high_water(
                &transaction,
                scenario_id,
                new_revision,
            )?;
            transaction.execute(
                "UPDATE scenario_history_state SET cursor_sequence = ?2 WHERE scenario_id = ?1",
                params![scenario_id.to_string(), u64_to_i64(new_cursor)?],
            )?;
            validate_global_identity_ownership(&transaction)?;
            increment_library_revision(&transaction)?;
            transaction.commit()?;
            Ok(CommittedCommand {
                new_revision,
                output,
            })
        })
        .await
    }

    /// Returns the command history in history-sequence order.
    ///
    /// # Errors
    ///
    /// Returns an error if the scenario is missing, the database actor or query
    /// fails, or persisted journal identifiers, timestamps, numeric values, or
    /// serialized command and actor data are invalid.
    pub async fn history(&self, scenario_id: ScenarioId) -> Result<Vec<HistoryEntry>, StoreError> {
        self.call(move |connection| {
            let exists = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM scenarios WHERE id = ?1)",
                [scenario_id.to_string()],
                |row| row.get::<_, bool>(0),
            )?;
            if !exists {
                return Err(StoreError::ScenarioNotFound(scenario_id));
            }
            let cursor = connection.query_row(
                "SELECT cursor_sequence FROM scenario_history_state WHERE scenario_id = ?1",
                [scenario_id.to_string()],
                |row| row.get::<_, i64>(0),
            )?;
            let cursor = i64_to_u64(cursor)?;
            let mut statement = connection.prepare(
                "SELECT id, revision_before, revision_after, command_type, command_json, inverse_json, actor_json, source, summary, created_at, history_sequence, branch_generation FROM command_journal WHERE scenario_id = ?1 ORDER BY history_sequence, branch_generation",
            )?;
            let mut rows = statement.query([scenario_id.to_string()])?;
            let mut entries = Vec::new();
            while let Some(row) = rows.next()? {
                entries.push(parse_history_row(row, cursor)?);
            }
            Ok(entries)
        })
        .await
    }

    /// Returns a typed application setting, if present.
    ///
    /// # Errors
    ///
    /// Returns an error if the actor or query fails, the stored setting cannot
    /// be deserialized as `T`, or its persisted timestamp is invalid.
    pub async fn get_setting<T: DeserializeOwned + Send + 'static>(
        &self,
        key: String,
    ) -> Result<Option<AppSetting<T>>, StoreError> {
        self.call(move |connection| {
            let row = connection
                .query_row(
                    "SELECT value_json, updated_at FROM app_settings WHERE key = ?1",
                    [key],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            row.map(|(json, updated_at)| {
                let value = serde_json::from_str(&json)?;
                let updated_at = parse_timestamp(&updated_at, "app setting updated_at")?;
                Ok(AppSetting { value, updated_at })
            })
            .transpose()
        })
        .await
    }

    /// Creates or replaces a typed application setting.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be serialized, the database actor
    /// or transaction fails, or the library revision cannot be represented.
    pub async fn set_setting<T: Serialize + Send + 'static>(
        &self,
        key: String,
        value: T,
        updated_at: Rfc3339Timestamp,
    ) -> Result<(), StoreError> {
        self.call(move |connection| {
            let value_json = serde_json::to_string(&value)?;
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute(
                "INSERT INTO app_settings (key, value_json, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
                params![key, value_json, updated_at.to_string()],
            )?;
            increment_library_revision(&transaction)?;
            transaction.commit()?;
            Ok(())
        })
        .await
    }

    /// Deletes an application setting and reports whether it existed.
    ///
    /// # Errors
    ///
    /// Returns an error if the database actor or transaction fails, or the
    /// library revision cannot be represented.
    pub async fn delete_setting(&self, key: String) -> Result<bool, StoreError> {
        self.call(move |connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let changed =
                transaction.execute("DELETE FROM app_settings WHERE key = ?1", [key])? != 0;
            if changed {
                increment_library_revision(&transaction)?;
            }
            transaction.commit()?;
            Ok(changed)
        })
        .await
    }
    /// Returns bounded, non-sensitive database diagnostics.
    ///
    /// # Errors
    ///
    /// Returns an error if the database actor is unavailable or any diagnostic
    /// pragma or schema query fails.
    pub async fn diagnostics(&self) -> Result<StoreDiagnostics, StoreError> {
        self.call(move |connection| {
            let foreign_keys =
                connection.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
            let journal_mode =
                connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
            let synchronous =
                connection.pragma_query_value(None, "synchronous", |row| row.get(0))?;
            let busy_timeout_ms =
                connection.pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
            let trusted_schema =
                connection.pragma_query_value(None, "trusted_schema", |row| row.get(0))?;
            let schema_version =
                connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
            let mut statement = connection.prepare(
                "SELECT type, name FROM sqlite_schema WHERE type IN ('table', 'index') AND name NOT LIKE 'sqlite_%' ORDER BY type, name",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut tables = Vec::new();
            let mut indexes = Vec::new();
            for row in rows {
                let (kind, name) = row?;
                if kind == "table" {
                    tables.push(name);
                } else {
                    indexes.push(name);
                }
            }
            Ok(StoreDiagnostics {
                foreign_keys,
                journal_mode,
                synchronous,
                busy_timeout_ms,
                trusted_schema,
                schema_version,
                tables,
                indexes,
            })
        })
        .await
    }

    /// Creates a transactionally consistent `SQLite` safety backup without
    /// exposing the live connection. The destination must not exist.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::BackupDestinationExists`] if the destination
    /// already exists. Also returns an error if the actor is unavailable or
    /// changing durability settings, copying the database, or restoring the
    /// live connection settings fails.
    pub async fn safety_backup(&self, destination: impl AsRef<Path>) -> Result<(), StoreError> {
        let destination = destination.as_ref().to_path_buf();
        self.call(move |connection| {
            let guard = prepare_private_new_file(&destination)?;
            connection.pragma_update(None, "synchronous", "FULL")?;
            let backup_result = connection.backup(MAIN_DB, &destination, None);
            let restore_result = connection.pragma_update(None, "synchronous", "NORMAL");
            if let Err(error) = backup_result {
                remove_guarded_file(&destination, &guard);
                restore_result?;
                return Err(StoreError::Database(error));
            }
            restore_result?;
            ensure_same_file(&guard.file, &destination)?;
            restrict_private_file(&destination)?;
            Ok(())
        })
        .await
    }

    /// Arms a one-shot transaction failpoint in debug/test builds.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ActorUnavailable`] if the failpoint state lock has
    /// been poisoned.
    #[cfg(debug_assertions)]
    pub fn set_failpoint(&self, failpoint_value: Failpoint) -> Result<(), StoreError> {
        let mut guard = self
            .failpoint
            .lock()
            .map_err(|_| StoreError::ActorUnavailable)?;
        *guard = Some(failpoint_value);
        Ok(())
    }
}

struct StagedApplyParts {
    import: StagedImport,
    remove_scenario_ids: BTreeSet<ScenarioId>,
    settings: BTreeMap<String, AppSetting<Value>>,
    authorization: Option<RestoreAuthorization>,
}

struct StagedScenarioOutcome {
    created: usize,
    replaced: usize,
    sources: Vec<ScenarioImportSource>,
}

#[derive(Clone, Copy)]
enum HistoryDirection {
    Undo,
    Redo,
}

struct PrivateFileGuard {
    file: File,
}

fn private_path_error(message: &'static str) -> StoreError {
    StoreError::PrivatePath(io::Error::other(message))
}

fn path_is_indirection(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

/// Creates or validates an owner-private application directory.
///
/// Every existing path component is checked for symlink/reparse indirection.
/// The requested directory is created with owner-only access when absent and
/// hardened and revalidated when present.
///
/// # Errors
///
/// Returns [`StoreError::PrivatePath`] when the path is root-like, indirect,
/// not a directory, has an unsafe ancestor, cannot be created, or cannot be
/// restricted to the current owner.
pub fn ensure_private_application_directory(path: impl AsRef<Path>) -> Result<(), StoreError> {
    ensure_private_parent(&path.as_ref().join(".eutheto-private-directory-guard"))
}

fn ensure_private_parent(path: &Path) -> Result<(), StoreError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if parent.file_name().is_none() && parent.has_root() {
        return Err(private_path_error(
            "private storage requires a dedicated parent directory",
        ));
    }
    if path.file_name().is_none() {
        return Err(private_path_error(
            "private storage path requires a regular file name",
        ));
    }
    let mut current = PathBuf::new();
    let components = parent.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {
                if current.as_os_str().is_empty() {
                    current.push(".");
                }
            }
            Component::ParentDir => {
                return Err(private_path_error(
                    "private storage paths may not contain parent-directory components",
                ));
            }
            Component::Normal(name) => {
                current.push(name);
                match std::fs::symlink_metadata(&current) {
                    Ok(metadata) => {
                        if path_is_indirection(&metadata) || !metadata.is_dir() {
                            return Err(private_path_error(
                                "private storage path contains an indirect or non-directory component",
                            ));
                        }
                        if index + 1 != components.len() {
                            ensure_safe_ancestor_directory(&current, &metadata)?;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        create_private_directory(&current)?;
                        restrict_private_directory(&current)?;
                    }
                    Err(error) => return Err(StoreError::PrivatePath(error)),
                }
            }
        }
        if index + 1 == components.len() {
            restrict_private_directory(&current)?;
        }
    }
    if components.is_empty() {
        restrict_private_directory(parent)?;
    }
    Ok(())
}

#[cfg(unix)]
fn ensure_safe_ancestor_directory(
    _path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = metadata.permissions().mode();
    if mode & 0o022 != 0 && mode & 0o1000 == 0 {
        return Err(private_path_error(
            "private storage path has an ambient writable ancestor",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn ensure_safe_ancestor_directory(
    path: &Path,
    _metadata: &std::fs::Metadata,
) -> Result<(), StoreError> {
    use std::process::{Command, Stdio};
    let script = r#"
$ErrorActionPreference = 'Stop'
$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$allowed = [System.Collections.Generic.HashSet[string]]::new()
$allowed.Add($identity.User.Value) | Out-Null
$allowed.Add('S-1-5-18') | Out-Null
$allowed.Add('S-1-5-32-544') | Out-Null
$writeMask = [System.Security.AccessControl.FileSystemRights]::DeleteSubdirectoriesAndFiles
$acl = Get-Acl -LiteralPath $args[0]
$rules = @($acl.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier]))
foreach ($rule in $rules) {
  if ($rule.AccessControlType -eq [System.Security.AccessControl.AccessControlType]::Allow -and
      -not $allowed.Contains($rule.IdentityReference.Value) -and
      (($rule.FileSystemRights -band $writeMask) -ne 0)) {
    throw 'private storage path has an ambient writable ancestor'
  }
}
"#;
    let status = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(StoreError::PrivatePath)?;
    if !status.success() {
        return Err(private_path_error(
            "private storage path has an ambient writable ancestor",
        ));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn ensure_safe_ancestor_directory(
    _path: &Path,
    _metadata: &std::fs::Metadata,
) -> Result<(), StoreError> {
    Err(private_path_error(
        "private storage is unsupported on this platform",
    ))
}

fn prepare_private_database_path(path: &Path) -> Result<PrivateFileGuard, StoreError> {
    ensure_private_parent(path)?;
    for sidecar in sqlite_sidecar_paths(path) {
        match std::fs::symlink_metadata(&sidecar) {
            Ok(metadata) => {
                ensure_regular_direct_file(&metadata)?;
                #[cfg(windows)]
                ensure_path_has_no_windows_hard_links(&sidecar)?;
                restrict_private_file(&sidecar)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(StoreError::PrivatePath(error)),
        }
    }
    prepare_private_file(path, false)
}

fn prepare_private_new_file(path: &Path) -> Result<PrivateFileGuard, StoreError> {
    ensure_private_parent(path)?;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure_regular_direct_file(&metadata)?;
            #[cfg(windows)]
            ensure_path_has_no_windows_hard_links(path)?;
            return Err(StoreError::BackupDestinationExists);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(StoreError::PrivatePath(error)),
    }
    prepare_private_file(path, true)
}

fn prepare_private_file(path: &Path, create_new: bool) -> Result<PrivateFileGuard, StoreError> {
    let mut options = FileOpenOptions::new();
    options.read(true).write(true);
    if create_new {
        options.create_new(true);
    } else {
        options.create(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|error| {
        if create_new && error.kind() == io::ErrorKind::AlreadyExists {
            StoreError::BackupDestinationExists
        } else {
            StoreError::PrivatePath(error)
        }
    })?;
    let metadata = file.metadata().map_err(StoreError::PrivatePath)?;
    ensure_regular_direct_file(&metadata)?;
    #[cfg(windows)]
    ensure_path_has_no_windows_hard_links(path)?;
    restrict_private_file(path)?;
    ensure_same_file(&file, path)?;
    Ok(PrivateFileGuard { file })
}

fn remove_guarded_file(path: &Path, guard: &PrivateFileGuard) {
    if ensure_same_file(&guard.file, path).is_ok() {
        let _ignored = std::fs::remove_file(path);
    }
}

fn ensure_regular_direct_file(metadata: &std::fs::Metadata) -> Result<(), StoreError> {
    if path_is_indirection(metadata) || !metadata.is_file() {
        return Err(private_path_error(
            "private storage path is indirect or is not a regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(private_path_error(
                "private storage files may not have additional hard links",
            ));
        }
    }
    Ok(())
}

fn verify_private_database_path(path: &Path, guard: &PrivateFileGuard) -> Result<(), StoreError> {
    ensure_same_file(&guard.file, path)?;
    restrict_private_file(path)?;
    for sidecar in sqlite_sidecar_paths(path) {
        match std::fs::symlink_metadata(&sidecar) {
            Ok(metadata) => {
                ensure_regular_direct_file(&metadata)?;
                restrict_private_file(&sidecar)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(StoreError::PrivatePath(error)),
        }
    }
    Ok(())
}

fn sqlite_sidecar_paths(path: &Path) -> [PathBuf; 3] {
    let raw = path.as_os_str().to_string_lossy();
    [
        PathBuf::from(format!("{raw}-wal")),
        PathBuf::from(format!("{raw}-shm")),
        PathBuf::from(format!("{raw}-journal")),
    ]
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700);
    builder.create(path).map_err(StoreError::PrivatePath)
}

#[cfg(windows)]
fn create_private_directory(path: &Path) -> Result<(), StoreError> {
    use std::process::{Command, Stdio};
    let script = r#"
$ErrorActionPreference = 'Stop'
$sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$acl = [System.Security.AccessControl.DirectorySecurity]::new()
$acl.SetAccessRuleProtection($true, $false)
$acl.SetOwner($sid)
$rule = [System.Security.AccessControl.FileSystemAccessRule]::new(
  $sid,
  [System.Security.AccessControl.FileSystemRights]::FullControl,
  [System.Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit',
  [System.Security.AccessControl.PropagationFlags]::None,
  [System.Security.AccessControl.AccessControlType]::Allow)
$acl.AddAccessRule($rule)
[System.IO.Directory]::CreateDirectory($args[0], $acl) | Out-Null
"#;
    let status = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(StoreError::PrivatePath)?;
    if !status.success() {
        return Err(private_path_error(
            "owner-private directory creation failed",
        ));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn create_private_directory(_path: &Path) -> Result<(), StoreError> {
    Err(private_path_error(
        "private storage is unsupported on this platform",
    ))
}

#[cfg(windows)]
fn ensure_path_has_no_windows_hard_links(path: &Path) -> Result<(), StoreError> {
    use std::process::{Command, Stdio};
    let script = r#"
$ErrorActionPreference = 'Stop'
$item = Get-Item -Force -LiteralPath $args[0]
if ($item.LinkType -eq 'HardLink') {
  throw 'private storage files may not have additional hard links'
}
"#;
    let status = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(StoreError::PrivatePath)?;
    if !status.success() {
        return Err(private_path_error(
            "private storage files may not have additional hard links",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_private_directory(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = std::fs::symlink_metadata(path).map_err(StoreError::PrivatePath)?;
    if path_is_indirection(&metadata)
        || !metadata.is_dir()
        || metadata.uid() != rustix::process::geteuid().as_raw()
    {
        return Err(private_path_error(
            "private storage directory ownership is invalid",
        ));
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(StoreError::PrivatePath)?;
    let metadata = std::fs::symlink_metadata(path).map_err(StoreError::PrivatePath)?;
    if path_is_indirection(&metadata)
        || !metadata.is_dir()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err(private_path_error(
            "private storage directory ownership or permissions are invalid",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_private_file(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = std::fs::symlink_metadata(path).map_err(StoreError::PrivatePath)?;
    ensure_regular_direct_file(&metadata)?;
    if metadata.uid() != rustix::process::geteuid().as_raw() {
        return Err(private_path_error(
            "private storage file ownership is invalid",
        ));
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(StoreError::PrivatePath)?;
    let metadata = std::fs::symlink_metadata(path).map_err(StoreError::PrivatePath)?;
    if path_is_indirection(&metadata)
        || !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err(private_path_error(
            "private storage file ownership or permissions are invalid",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn restrict_private_directory(path: &Path) -> Result<(), StoreError> {
    restrict_windows_acl(path, true)
}

#[cfg(windows)]
fn restrict_private_file(path: &Path) -> Result<(), StoreError> {
    restrict_windows_acl(path, false)
}

#[cfg(windows)]
fn restrict_windows_acl(path: &Path, directory: bool) -> Result<(), StoreError> {
    use std::process::{Command, Stdio};
    let script = r#"
$ErrorActionPreference = 'Stop'
$path = $args[0]
$isDirectory = $args[1] -eq 'directory'
$sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$item = Get-Item -Force -LiteralPath $path
if (-not $isDirectory -and $item.LinkType -eq 'HardLink') {
  throw 'private storage files may not have additional hard links'
}
$existingAcl = Get-Acl -LiteralPath $path
if ($existingAcl.GetOwner([System.Security.Principal.SecurityIdentifier]) -ne $sid) {
  throw 'private storage ownership is invalid'
}
if ($isDirectory) {
  $acl = [System.Security.AccessControl.DirectorySecurity]::new()
  $inheritance = [System.Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'
} else {
  $acl = [System.Security.AccessControl.FileSecurity]::new()
  $inheritance = [System.Security.AccessControl.InheritanceFlags]::None
}
$acl.SetAccessRuleProtection($true, $false)
$acl.SetOwner($sid)
$rule = [System.Security.AccessControl.FileSystemAccessRule]::new(
  $sid,
  [System.Security.AccessControl.FileSystemRights]::FullControl,
  $inheritance,
  [System.Security.AccessControl.PropagationFlags]::None,
  [System.Security.AccessControl.AccessControlType]::Allow)
$acl.AddAccessRule($rule)
Set-Acl -LiteralPath $path -AclObject $acl
$verified = Get-Acl -LiteralPath $path
$rules = @($verified.GetAccessRules($true, $false, [System.Security.Principal.SecurityIdentifier]))
if (-not $verified.AreAccessRulesProtected -or $rules.Count -ne 1 -or
    $rules[0].IdentityReference -ne $sid -or
    $rules[0].AccessControlType -ne [System.Security.AccessControl.AccessControlType]::Allow -or
    (($rules[0].FileSystemRights -band [System.Security.AccessControl.FileSystemRights]::FullControl) -ne
      [System.Security.AccessControl.FileSystemRights]::FullControl)) {
  throw 'owner-private ACL verification failed'
}
"#;
    let status = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .arg(path)
        .arg(if directory { "directory" } else { "file" })
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(StoreError::PrivatePath)?;
    if !status.success() {
        return Err(private_path_error(
            "current-user-only ACL creation or verification failed",
        ));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn restrict_private_directory(_path: &Path) -> Result<(), StoreError> {
    Err(private_path_error(
        "private storage is unsupported on this platform",
    ))
}

#[cfg(not(any(unix, windows)))]
fn restrict_private_file(_path: &Path) -> Result<(), StoreError> {
    Err(private_path_error(
        "private storage is unsupported on this platform",
    ))
}

fn ensure_same_file(file: &File, path: &Path) -> Result<(), StoreError> {
    let selected = std::fs::symlink_metadata(path).map_err(StoreError::PrivatePath)?;
    ensure_regular_direct_file(&selected)?;
    let opened = same_file::Handle::from_file(file.try_clone().map_err(StoreError::PrivatePath)?)
        .map_err(StoreError::PrivatePath)?;
    let selected = same_file::Handle::from_path(path).map_err(StoreError::PrivatePath)?;
    if opened != selected {
        return Err(private_path_error(
            "private storage file changed during validation",
        ));
    }
    Ok(())
}

fn preflight_schema_version(file: &File) -> Result<(), StoreError> {
    let mut file = file.try_clone().map_err(|error| {
        StoreError::Integrity(format!("database schema preflight failed: {error}"))
    })?;
    if file
        .metadata()
        .map_err(|error| {
            StoreError::Integrity(format!("database schema preflight failed: {error}"))
        })?
        .len()
        < 64
    {
        return Ok(());
    }
    let mut header = [0_u8; 64];
    file.read_exact(&mut header).map_err(|error| {
        StoreError::Integrity(format!("database schema preflight failed: {error}"))
    })?;
    if &header[..16] != b"SQLite format 3\0" {
        return Ok(());
    }
    let found = u32::from_be_bytes(header[60..64].try_into().map_err(|_| {
        StoreError::Integrity("database schema preflight header is invalid".to_owned())
    })?);
    if found > CURRENT_SCHEMA_VERSION {
        return Err(StoreError::NewerSchema {
            found,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    Ok(())
}

fn open_connection(path: &Path) -> Result<Connection, StoreError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.pragma_update(None, "trusted_schema", "OFF")?;
    Ok(connection)
}

fn initialize_connection(
    connection: &mut Connection,
    #[cfg(debug_assertions)] failpoint: &Arc<std::sync::Mutex<Option<Failpoint>>>,
) -> Result<InitializationOutcome, StoreError> {
    let applied_migrations = initialize_schema(
        connection,
        #[cfg(debug_assertions)]
        failpoint,
    )?;

    let provenance_policy: String = connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key = 'portable_import_provenance_retention_policy'",
            [],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            StoreError::Integrity(
                "portable import provenance retention policy is missing".to_owned(),
            )
        })?;
    if provenance_policy.parse::<u32>().ok() != Some(IMPORT_PROVENANCE_RETENTION_POLICY_VERSION) {
        return Err(StoreError::Integrity(
            "portable import provenance retention policy is unsupported".to_owned(),
        ));
    }
    initialize_scenario_revision_high_water(connection)?;
    initialize_scenario_identity_owners(connection)?;

    let quick_check: String =
        connection.pragma_query_value(None, "quick_check", |row| row.get(0))?;
    if quick_check != "ok" {
        return Err(StoreError::Integrity(quick_check));
    }
    let interrupted_at = jiff::Timestamp::now().to_string();
    let mut statement = connection.prepare(
        "UPDATE solve_runs SET status = 'interrupted', finished_at = COALESCE(finished_at, ?1), error_json = COALESCE(error_json, '{\"code\":\"interrupted\"}') WHERE status = 'running' RETURNING id",
    )?;
    let rows = statement.query_map([interrupted_at], |row| row.get::<_, String>(0))?;
    let mut interrupted_solve_run_ids = Vec::new();
    for row in rows {
        let id = row?;
        interrupted_solve_run_ids.push(id.parse().map_err(|error| {
            StoreError::Integrity(format!("invalid stored solve-run id: {error}"))
        })?);
    }
    Ok(InitializationOutcome {
        schema_version: CURRENT_SCHEMA_VERSION,
        applied_migrations,
        integrity: IntegrityOutcome::Ok,
        recovery: RecoveryOutcome {
            interrupted_solve_run_ids,
        },
    })
}

fn initialize_scenario_revision_high_water(connection: &mut Connection) -> Result<(), StoreError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut statement = transaction.prepare(
        "SELECT scenario_id, MAX(revision) FROM (
            SELECT id AS scenario_id, revision FROM scenarios
            UNION ALL
            SELECT scenario_id, revision FROM retained_scenario_revisions
            UNION ALL
            SELECT scenario_id, revision FROM scenario_snapshots
            UNION ALL
            SELECT scenario_id, revision_after AS revision FROM command_journal
        ) GROUP BY scenario_id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for (scenario_id, revision) in rows {
        let scenario_id = scenario_id.parse().map_err(|error| {
            StoreError::Integrity(format!(
                "invalid scenario identity while backfilling revision high-water: {error}"
            ))
        })?;
        record_scenario_revision_high_water(
            &transaction,
            scenario_id,
            checked_revision(i64_to_u64(revision)?)?,
        )?;
    }
    load_scenario_revision_high_water(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn initialize_scenario_identity_owners(connection: &mut Connection) -> Result<(), StoreError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for (scenario_id, _) in load_scenario_revision_high_water(&transaction)? {
        record_scenario_identity_owners(
            &transaction,
            scenario_id,
            &BTreeSet::from([scenario_id.as_uuid()]),
        )?;
    }
    for scenario_id in scenario_revisions(&transaction)?.keys().copied() {
        let project = load_project(&transaction, scenario_id)?;
        record_scenario_identity_owners(
            &transaction,
            scenario_id,
            &collect_project_owned_uuids(
                &project.document,
                &project.portable.semantic_extensions,
                &project.portable.extensions,
            ),
        )?;
    }
    for scenario in read_retained_scenario_revision_rows(&transaction)?.into_values() {
        record_scenario_identity_owners(
            &transaction,
            scenario.document.scenario_id,
            &collect_project_owned_uuids(
                &scenario.document,
                &scenario.semantic_extensions,
                &scenario.extensions,
            ),
        )?;
    }
    load_scenario_identity_owners(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn initialize_schema(
    connection: &mut Connection,
    #[cfg(debug_assertions)] failpoint: &Arc<std::sync::Mutex<Option<Failpoint>>>,
) -> Result<Vec<u32>, StoreError> {
    let found_version: u32 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if found_version > CURRENT_SCHEMA_VERSION {
        return Err(StoreError::NewerSchema {
            found: found_version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }

    let checksum = blake3::hash(INITIAL_MIGRATION.as_bytes())
        .to_hex()
        .to_string();
    if found_version == 0 {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, checksum TEXT NOT NULL, applied_at TEXT NOT NULL) STRICT;",
        )?;
        transaction.execute_batch(INITIAL_MIGRATION)?;
        #[cfg(debug_assertions)]
        consume_failpoint(failpoint, Failpoint::AfterMigrationSql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (1, 'V1_initial_schema.sql', ?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [checksum],
        )?;
        transaction.pragma_update(None, "user_version", 1)?;
        transaction.commit()?;
        return Ok(vec![1]);
    }

    let has_registry: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    if !has_registry {
        return Err(StoreError::Integrity(
            "database schema migration registry is missing".to_owned(),
        ));
    }
    let registry_version: Option<u32> =
        connection.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })?;
    let registry_version = registry_version.unwrap_or(0);
    if registry_version > CURRENT_SCHEMA_VERSION {
        return Err(StoreError::NewerSchema {
            found: registry_version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    let stored_checksum = connection
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| StoreError::Integrity("initial database migration is missing".to_owned()))?;
    if stored_checksum != checksum {
        return Err(StoreError::MigrationChanged { version: 1 });
    }
    Ok(Vec::new())
}

async fn update_archive(
    store: &SqliteScenarioStore,
    scenario_id: ScenarioId,
    expected_revision: Revision,
    archived_at: Option<Rfc3339Timestamp>,
) -> Result<(), StoreError> {
    store
        .call(move |connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let actual_revision = scenario_revision(&transaction, scenario_id)?;
            ensure_revision(expected_revision, actual_revision.value())?;
            transaction.execute(
                "UPDATE scenarios SET archived_at = ?2 WHERE id = ?1",
                params![
                    scenario_id.to_string(),
                    archived_at.map(|timestamp| timestamp.to_string())
                ],
            )?;
            increment_library_revision(&transaction)?;
            transaction.commit()?;
            Ok(())
        })
        .await
}

fn scenario_revision(
    connection: &Connection,
    scenario_id: ScenarioId,
) -> Result<Revision, StoreError> {
    let revision = connection
        .query_row(
            "SELECT revision FROM scenarios WHERE id = ?1",
            [scenario_id.to_string()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or(StoreError::ScenarioNotFound(scenario_id))?;
    checked_revision(i64_to_u64(revision)?)
}

fn scenario_archived_at(
    connection: &Connection,
    scenario_id: ScenarioId,
) -> Result<Option<Rfc3339Timestamp>, StoreError> {
    let archived_at = connection
        .query_row(
            "SELECT archived_at FROM scenarios WHERE id = ?1",
            [scenario_id.to_string()],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    parse_optional_timestamp(archived_at, "scenario archived_at")
}

fn load_project(
    connection: &Connection,
    scenario_id: ScenarioId,
) -> Result<StoredProject, StoreError> {
    let row = connection
        .query_row(
            "SELECT id, domain_pack_id, domain_schema_version, title, description, revision, created_at, updated_at, last_opened_at, archived_at, document_json, portable_required_capabilities_json, portable_semantic_extensions_json, portable_extensions_json FROM scenarios WHERE id = ?1",
            [scenario_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, String>(13)?,
                ))
            },
        )
        .optional()?;
    let Some((
        id,
        pack,
        schema,
        title,
        description,
        revision,
        created_at,
        updated_at,
        last_opened_at,
        archived_at,
        document_json,
        required_capabilities_json,
        semantic_extensions_json,
        extensions_json,
    )) = row
    else {
        return Err(StoreError::ScenarioNotFound(scenario_id));
    };
    let id = id
        .parse()
        .map_err(|error| StoreError::Integrity(format!("invalid stored scenario id: {error}")))?;
    let domain_pack_id = PackId::new(&pack)
        .map_err(|error| StoreError::Integrity(format!("invalid stored pack id: {error}")))?;
    let document_value: Value = serde_json::from_str(&document_json)?;
    validate_stored_portable_json(&document_value, "scenario document")?;
    let semantic_extensions_value: Value = serde_json::from_str(&semantic_extensions_json)?;
    validate_stored_portable_json(&semantic_extensions_value, "scenario semantic extensions")?;
    let extensions_value: Value = serde_json::from_str(&extensions_json)?;
    validate_stored_portable_json(&extensions_value, "scenario extensions")?;
    let project = StoredProject {
        summary: ProjectSummary {
            id,
            domain_pack_id,
            domain_schema_version: i64_to_u32(schema)?,
            title,
            description,
            revision: checked_revision(i64_to_u64(revision)?)?,
            created_at: parse_timestamp(&created_at, "scenario created_at")?,
            updated_at: parse_timestamp(&updated_at, "scenario updated_at")?,
            last_opened_at: parse_optional_timestamp(last_opened_at, "scenario last_opened_at")?,
            archived_at: parse_optional_timestamp(archived_at, "scenario archived_at")?,
        },
        document: serde_json::from_value(document_value)?,
        portable: PortableWrapperMetadata {
            required_capabilities: serde_json::from_str(&required_capabilities_json)?,
            semantic_extensions: serde_json::from_value(semantic_extensions_value)?,
            extensions: serde_json::from_value(extensions_value)?,
        },
    };
    validate_project_owned_uuid_uniqueness(
        &project.document,
        project.summary.revision,
        &project.portable,
    )
    .map_err(StoreError::Integrity)?;
    Ok(project)
}

fn load_retained_scenario_revisions(
    connection: &Connection,
) -> Result<Vec<StoredScenarioRevision>, StoreError> {
    let dependencies = retained_result_dependencies(connection)?;
    let current = scenario_revisions(connection)?;
    let retained = read_retained_scenario_revision_rows(connection)?;
    for identity in retained.keys() {
        if !dependencies.contains(identity) {
            return Err(StoreError::Integrity(
                "an immutable scenario revision is not required by a retained result".to_owned(),
            ));
        }
    }
    let mut historical = Vec::new();
    for dependency in dependencies {
        if current.get(&dependency.scenario_id) == Some(&dependency.scenario_revision) {
            continue;
        }
        let scenario = retained.get(&dependency).cloned().ok_or_else(|| {
            StoreError::Integrity(format!(
                "retained result requires unavailable scenario {} revision {}",
                dependency.scenario_id,
                dependency.scenario_revision.value()
            ))
        })?;
        historical.push(StoredScenarioRevision { scenario });
    }
    Ok(historical)
}

fn synchronize_retained_scenario_revisions(
    transaction: &rusqlite::Transaction<'_>,
    staged: Vec<PortableScenario>,
) -> Result<(), StoreError> {
    let mut candidates = BTreeMap::new();
    for scenario in staged {
        validate_scenario_owned_uuid_uniqueness(&scenario)
            .map_err(|error| StoreError::InvalidStagedApply(error.to_string()))?;
        serialize_document(&scenario.document)?;
        validate_historical_scenario(&scenario).map_err(StoreError::InvalidStagedApply)?;
        let identity = ScenarioRevisionReference {
            scenario_id: scenario.document.scenario_id,
            scenario_revision: scenario.revision,
        };
        if let Some(existing) = candidates.insert(identity, scenario.clone())
            && existing != scenario
        {
            return Err(StoreError::InvalidStagedApply(
                "staged input contains conflicting immutable scenario revisions".to_owned(),
            ));
        }
    }
    for scenario in candidates.values() {
        insert_retained_scenario_revision(transaction, scenario)?;
    }

    let dependencies = retained_result_dependencies(transaction)?;
    let retained = read_retained_scenario_revision_rows(transaction)?;
    for dependency in &dependencies {
        if retained.contains_key(dependency) {
            continue;
        }
        let project =
            load_project(transaction, dependency.scenario_id).map_err(|error| match error {
                StoreError::ScenarioNotFound(_) => StoreError::InvalidStagedApply(format!(
                    "retained result requires missing scenario {}",
                    dependency.scenario_id
                )),
                other => other,
            })?;
        if project.summary.revision != dependency.scenario_revision {
            return Err(StoreError::InvalidStagedApply(format!(
                "retained result requires unavailable scenario {} revision {}",
                dependency.scenario_id,
                dependency.scenario_revision.value()
            )));
        }
        let scenario = historical_scenario_from_project(project);
        insert_retained_scenario_revision(transaction, &scenario)?;
    }

    let retained = read_retained_scenario_revision_rows(transaction)?;
    let mut delete = transaction.prepare(
        "DELETE FROM retained_scenario_revisions WHERE scenario_id = ?1 AND revision = ?2",
    )?;
    for identity in retained.keys() {
        if !dependencies.contains(identity) {
            delete.execute(params![
                identity.scenario_id.to_string(),
                u64_to_i64(identity.scenario_revision.value())?
            ])?;
        }
    }
    Ok(())
}

fn historical_scenario_from_project(project: StoredProject) -> PortableScenario {
    let mut scenario = PortableScenario::current(
        project.summary.revision,
        project.document,
        project.portable.required_capabilities,
    );
    scenario.semantic_extensions = project.portable.semantic_extensions;
    scenario.extensions = project.portable.extensions;
    scenario
}

fn validate_historical_scenario(scenario: &PortableScenario) -> Result<(), String> {
    if scenario.project.is_some() {
        return Err("immutable scenario revisions may not contain project metadata".to_owned());
    }
    eutheto_export::validate_current_portable_scenario(scenario)
        .map_err(|error| format!("immutable scenario revision is invalid: {error}"))
}

fn insert_retained_scenario_revision(
    connection: &Connection,
    scenario: &PortableScenario,
) -> Result<(), StoreError> {
    let scenario_id = scenario.document.scenario_id;
    let revision = scenario.revision;
    let serialized = serde_json::to_string(scenario)?;
    let existing = connection
        .query_row(
            "SELECT scenario_json FROM retained_scenario_revisions WHERE scenario_id = ?1 AND revision = ?2",
            params![scenario_id.to_string(), u64_to_i64(revision.value())?],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        let existing: PortableScenario = serde_json::from_str(&existing)?;
        if &existing != scenario {
            return Err(StoreError::InvalidStagedApply(format!(
                "immutable scenario {} revision {} conflicts with retained content",
                scenario_id,
                revision.value()
            )));
        }
        record_scenario_revision_high_water(connection, scenario_id, revision)?;
        return Ok(());
    }
    connection.execute(
        "INSERT INTO retained_scenario_revisions (scenario_id, revision, scenario_json) VALUES (?1, ?2, ?3)",
        params![
            scenario_id.to_string(),
            u64_to_i64(revision.value())?,
            serialized
        ],
    )?;
    record_scenario_revision_high_water(connection, scenario_id, revision)?;
    Ok(())
}

fn read_retained_scenario_revision_rows(
    connection: &Connection,
) -> Result<BTreeMap<ScenarioRevisionReference, PortableScenario>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT scenario_id, revision, scenario_json FROM retained_scenario_revisions ORDER BY scenario_id, revision",
    )?;
    let mut rows = statement.query([])?;
    let mut revisions = BTreeMap::new();
    while let Some(row) = rows.next()? {
        let scenario_id_text: String = row.get(0)?;
        let scenario_id = scenario_id_text.parse().map_err(|error| {
            StoreError::Integrity(format!(
                "invalid retained scenario revision identity: {error}"
            ))
        })?;
        let revision = checked_revision(i64_to_u64(row.get(1)?)?)?;
        let json: String = row.get(2)?;
        let scenario: PortableScenario = serde_json::from_str(&json)?;
        validate_historical_scenario(&scenario).map_err(StoreError::Integrity)?;
        if scenario.document.scenario_id != scenario_id || scenario.revision != revision {
            return Err(StoreError::Integrity(
                "retained scenario revision columns disagree with its document".to_owned(),
            ));
        }
        revisions.insert(
            ScenarioRevisionReference {
                scenario_id,
                scenario_revision: revision,
            },
            scenario,
        );
    }
    Ok(revisions)
}

fn retained_result_dependencies(
    connection: &Connection,
) -> Result<BTreeSet<ScenarioRevisionReference>, StoreError> {
    let mut statement = connection
        .prepare("SELECT value FROM portable_sections WHERE section = ?1 ORDER BY key")?;
    let mut rows = statement.query([SupplementalSectionKind::Results.as_str()])?;
    let mut dependencies = BTreeSet::new();
    while let Some(row) = rows.next()? {
        let bytes: Vec<u8> = row.get(0)?;
        let value: Value = serde_json::from_slice(&bytes)?;
        let dependency = extract_result_dependency(&value).map_err(|error| {
            StoreError::Integrity(format!(
                "stored retained result has an invalid scenario revision reference: {error}"
            ))
        })?;
        dependencies.insert(dependency);
    }
    Ok(dependencies)
}

fn read_summary_row(row: &rusqlite::Row<'_>) -> Result<ProjectSummary, StoreError> {
    let id_text: String = row.get(0)?;
    let id = id_text
        .parse()
        .map_err(|error| StoreError::Integrity(format!("invalid stored scenario id: {error}")))?;
    let pack_text: String = row.get(1)?;
    let domain_pack_id = PackId::new(&pack_text)
        .map_err(|error| StoreError::Integrity(format!("invalid stored pack id: {error}")))?;
    let created_at: String = row.get(6)?;
    let updated_at: String = row.get(7)?;
    let last_opened_at: Option<String> = row.get(8)?;
    let archived_at: Option<String> = row.get(9)?;
    Ok(ProjectSummary {
        id,
        domain_pack_id,
        domain_schema_version: i64_to_u32(row.get(2)?)?,
        title: row.get(3)?,
        description: row.get(4)?,
        revision: checked_revision(i64_to_u64(row.get(5)?)?)?,
        created_at: parse_timestamp(&created_at, "scenario created_at")?,
        updated_at: parse_timestamp(&updated_at, "scenario updated_at")?,
        last_opened_at: parse_optional_timestamp(last_opened_at, "scenario last_opened_at")?,
        archived_at: parse_optional_timestamp(archived_at, "scenario archived_at")?,
    })
}

fn history_state(
    transaction: &rusqlite::Transaction<'_>,
    scenario_id: ScenarioId,
) -> Result<(u64, u64, u64), StoreError> {
    let (cursor, generation) = transaction.query_row(
        "SELECT cursor_sequence, branch_generation FROM scenario_history_state WHERE scenario_id = ?1",
        [scenario_id.to_string()],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )?;
    let max_sequence: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(history_sequence), 0) FROM command_journal WHERE scenario_id = ?1",
        [scenario_id.to_string()],
        |row| row.get(0),
    )?;
    Ok((
        i64_to_u64(cursor)?,
        i64_to_u64(generation)?,
        i64_to_u64(max_sequence)?,
    ))
}

fn load_history_entry(
    transaction: &rusqlite::Transaction<'_>,
    scenario_id: ScenarioId,
    sequence: u64,
    cursor: u64,
) -> Result<HistoryEntry, StoreError> {
    let mut statement = transaction.prepare(
        "SELECT id, revision_before, revision_after, command_type, command_json, inverse_json, actor_json, source, summary, created_at, history_sequence, branch_generation FROM command_journal WHERE scenario_id = ?1 AND history_sequence = ?2 ORDER BY branch_generation DESC LIMIT 1",
    )?;
    let mut rows = statement.query(params![scenario_id.to_string(), u64_to_i64(sequence)?])?;
    let Some(row) = rows.next()? else {
        return Err(StoreError::Integrity(
            "history cursor references a missing journal entry".to_owned(),
        ));
    };
    parse_history_row(row, cursor)
}

fn parse_history_row(row: &rusqlite::Row<'_>, cursor: u64) -> Result<HistoryEntry, StoreError> {
    let sequence = i64_to_u64(row.get(10)?)?;
    let id_text: String = row.get(0)?;
    let id = id_text
        .parse()
        .map_err(|error| StoreError::Integrity(format!("invalid stored command id: {error}")))?;
    let source_text: String = row.get(7)?;
    let created_at: String = row.get(9)?;
    Ok(HistoryEntry {
        id,
        revision_before: checked_revision(i64_to_u64(row.get(1)?)?)?,
        revision_after: checked_revision(i64_to_u64(row.get(2)?)?)?,
        command_type: row.get(3)?,
        command: serde_json::from_str(&row.get::<_, String>(4)?)?,
        inverse: row
            .get::<_, Option<String>>(5)?
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
        actor: serde_json::from_str(&row.get::<_, String>(6)?)?,
        source: parse_command_source(&source_text)?,
        summary: row.get(8)?,
        created_at: parse_timestamp(&created_at, "journal created_at")?,
        history_sequence: sequence,
        branch_generation: i64_to_u64(row.get(11)?)?,
        applied: sequence <= cursor,
    })
}

fn maybe_snapshot(
    transaction: &rusqlite::Transaction<'_>,
    scenario_id: ScenarioId,
    revision: u64,
    sequence: u64,
    document: &ScenarioDocument,
    created_at: &Rfc3339Timestamp,
    policy: SnapshotPolicy,
) -> Result<(), StoreError> {
    if !sequence.is_multiple_of(u64::from(policy.interval.get())) {
        return Ok(());
    }
    let bytes = serde_json::to_vec(document)?;
    if u64::try_from(bytes.len()).map_err(|_| StoreError::NumericRange)? > policy.max_document_bytes
    {
        return Err(StoreError::SnapshotTooLarge);
    }
    let compressed = zstd::stream::encode_all(bytes.as_slice(), policy.compression_level)
        .map_err(StoreError::Compression)?;
    transaction.execute(
        "INSERT INTO scenario_snapshots (id, scenario_id, revision, document_json_zstd, created_at, reason) VALUES (?1, ?2, ?3, ?4, ?5, 'periodic')",
        params![Uuid::now_v7().to_string(), scenario_id.to_string(), u64_to_i64(revision)?, compressed, created_at.to_string()],
    )?;
    Ok(())
}

struct DocumentProjection {
    domain_pack_id: String,
    domain_schema_version: u32,
    title: String,
    description: Option<String>,
    created_at: String,
    updated_at: String,
}

fn document_projection(document: &ScenarioDocument) -> DocumentProjection {
    DocumentProjection {
        domain_pack_id: document.domain_pack.id.to_string(),
        domain_schema_version: document.domain_pack.schema_version,
        title: document.metadata.title.clone(),
        description: Some(document.metadata.description.clone()),
        created_at: document.metadata.created_at.to_string(),
        updated_at: document.metadata.updated_at.to_string(),
    }
}

fn duplicate_project_contents(
    source: StoredProject,
    new_id: ScenarioId,
    mapping: &BTreeMap<Uuid, Uuid>,
    occupied: &BTreeSet<Uuid>,
    title: &str,
    timestamp: Rfc3339Timestamp,
) -> Result<(ScenarioDocument, PortableWrapperMetadata), StoreError> {
    let owned = collect_project_owned_uuids(
        &source.document,
        &source.portable.semantic_extensions,
        &source.portable.extensions,
    );
    let mapped_keys = mapping.keys().copied().collect::<BTreeSet<_>>();
    if mapped_keys != owned {
        return Err(StoreError::InvalidDuplicateMapping(
            "mapping keys do not exactly match source-owned identities".to_owned(),
        ));
    }
    if mapping.get(&source.document.scenario_id.as_uuid()) != Some(&new_id.as_uuid()) {
        return Err(StoreError::InvalidDuplicateMapping(
            "source scenario identity does not map to the requested destination".to_owned(),
        ));
    }
    let mapped_values = mapping.values().copied().collect::<BTreeSet<_>>();
    if mapped_values.len() != mapping.len() {
        return Err(StoreError::InvalidDuplicateMapping(
            "mapping target identities are not unique".to_owned(),
        ));
    }
    if mapped_values
        .iter()
        .any(|identity| identity.get_version_num() != 7)
    {
        return Err(StoreError::InvalidDuplicateMapping(
            "mapping target identities must be UUIDv7".to_owned(),
        ));
    }
    if mapped_values
        .iter()
        .any(|identity| occupied.contains(identity))
    {
        return Err(StoreError::InvalidDuplicateMapping(
            "mapping target identity is already occupied".to_owned(),
        ));
    }

    let mut domain = serde_json::to_value(&source.document.domain)?;
    let mut semantic_extensions = serde_json::to_value(&source.portable.semantic_extensions)?;
    let mut document_extensions = serde_json::to_value(&source.document.extensions)?;
    let mut portable_extensions = serde_json::to_value(&source.portable.extensions)?;
    rewrite_domain_definitions(&mut domain, mapping)?;
    rewrite_self_declared_definitions(&mut domain, mapping)?;
    rewrite_self_declared_definitions(&mut semantic_extensions, mapping)?;
    rewrite_self_declared_definitions(&mut document_extensions, mapping)?;
    rewrite_self_declared_definitions(&mut portable_extensions, mapping)?;
    rewrite_declared_references(&mut domain, mapping)?;
    rewrite_declared_references(&mut semantic_extensions, mapping)?;
    rewrite_declared_references(&mut document_extensions, mapping)?;
    rewrite_declared_references(&mut portable_extensions, mapping)?;
    reject_stale_declared_references(&domain, mapping)?;
    reject_stale_declared_references(&semantic_extensions, mapping)?;
    reject_stale_declared_references(&document_extensions, mapping)?;
    reject_stale_declared_references(&portable_extensions, mapping)?;

    let mut document = source.document;
    document.scenario_id = new_id;
    title.clone_into(&mut document.metadata.title);
    document.metadata.created_at = timestamp;
    document.metadata.updated_at = timestamp;
    document.domain = serde_json::from_value(domain)?;
    document.extensions = serde_json::from_value(document_extensions)?;
    let mut portable = source.portable;
    portable.semantic_extensions = serde_json::from_value(semantic_extensions)?;
    portable.extensions = serde_json::from_value(portable_extensions)?;
    Ok((document, portable))
}

fn collect_project_owned_uuids(
    document: &ScenarioDocument,
    semantic_extensions: &BTreeMap<String, Value>,
    extensions: &BTreeMap<String, Value>,
) -> BTreeSet<Uuid> {
    let mut owned = BTreeSet::from([document.scenario_id.as_uuid()]);
    owned.extend(
        document
            .domain
            .entities
            .keys()
            .map(|id| id.as_uuid())
            .chain(document.domain.rules.keys().map(|id| id.as_uuid()))
            .chain(document.domain.preferences.keys().map(|id| id.as_uuid()))
            .chain(
                document
                    .domain
                    .locked_assignments
                    .keys()
                    .map(|id| id.as_uuid()),
            ),
    );
    for value in document
        .domain
        .entities
        .values()
        .chain(document.domain.rules.values())
        .chain(document.domain.preferences.values())
        .chain(document.domain.locked_assignments.values())
        .chain(semantic_extensions.values())
        .chain(document.extensions.values())
        .chain(extensions.values())
    {
        owned.extend(collect_self_declared_uuids(value));
    }
    owned
}

fn authoritative_occupied_uuids(connection: &Connection) -> Result<BTreeSet<Uuid>, StoreError> {
    let mut occupied = BTreeSet::new();
    occupied.extend(load_scenario_identity_owners(connection)?.keys().copied());
    occupied.extend(
        load_scenario_revision_high_water(connection)?
            .keys()
            .map(|scenario_id| scenario_id.as_uuid()),
    );
    for scenario_id in scenario_revisions(connection)?.keys() {
        let project = load_project(connection, *scenario_id)?;
        occupied.extend(collect_project_owned_uuids(
            &project.document,
            &project.portable.semantic_extensions,
            &project.portable.extensions,
        ));
    }
    for scenario in read_retained_scenario_revision_rows(connection)?.into_values() {
        occupied.extend(collect_project_owned_uuids(
            &scenario.document,
            &scenario.semantic_extensions,
            &scenario.extensions,
        ));
    }
    occupied.extend(
        load_supplemental_identity_owners(connection)?
            .keys()
            .copied(),
    );
    Ok(occupied)
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum StoredIdentityOwner {
    ScenarioFamily(ScenarioId),
    Supplemental(SupplementalIdentity),
}

fn register_stored_identity(
    owners: &mut BTreeMap<Uuid, StoredIdentityOwner>,
    identity: Uuid,
    owner: StoredIdentityOwner,
) -> Result<(), StoreError> {
    if let Some(existing) = owners.get(&identity) {
        if matches!(
            (existing, &owner),
            (
                StoredIdentityOwner::ScenarioFamily(left),
                StoredIdentityOwner::ScenarioFamily(right)
            ) if left == right
        ) {
            return Ok(());
        }
        return Err(StoreError::IdentityCollision(identity));
    }
    owners.insert(identity, owner);
    Ok(())
}

fn validate_global_identity_ownership(connection: &Connection) -> Result<(), StoreError> {
    let mut owners = BTreeMap::new();
    for (identity, scenario_id) in load_scenario_identity_owners(connection)? {
        register_stored_identity(
            &mut owners,
            identity,
            StoredIdentityOwner::ScenarioFamily(scenario_id),
        )?;
    }
    for scenario_id in load_scenario_revision_high_water(connection)?
        .keys()
        .copied()
    {
        register_stored_identity(
            &mut owners,
            scenario_id.as_uuid(),
            StoredIdentityOwner::ScenarioFamily(scenario_id),
        )?;
    }
    for scenario_id in scenario_revisions(connection)?.keys().copied() {
        let project = load_project(connection, scenario_id)?;
        for identity in collect_project_owned_uuids(
            &project.document,
            &project.portable.semantic_extensions,
            &project.portable.extensions,
        ) {
            register_stored_identity(
                &mut owners,
                identity,
                StoredIdentityOwner::ScenarioFamily(scenario_id),
            )?;
        }
    }
    for scenario in read_retained_scenario_revision_rows(connection)?.into_values() {
        let scenario_id = scenario.document.scenario_id;
        for identity in collect_project_owned_uuids(
            &scenario.document,
            &scenario.semantic_extensions,
            &scenario.extensions,
        ) {
            register_stored_identity(
                &mut owners,
                identity,
                StoredIdentityOwner::ScenarioFamily(scenario_id),
            )?;
        }
    }
    for (identity, supplemental) in load_supplemental_identity_owners(connection)? {
        register_stored_identity(
            &mut owners,
            identity,
            StoredIdentityOwner::Supplemental(supplemental),
        )?;
    }
    Ok(())
}

fn rewrite_domain_definitions(
    domain: &mut Value,
    mapping: &BTreeMap<Uuid, Uuid>,
) -> Result<(), StoreError> {
    let object = domain
        .as_object_mut()
        .ok_or_else(|| StoreError::Integrity("scenario domain is not an object".to_owned()))?;
    for section in ["entities", "rules", "preferences", "lockedAssignments"] {
        let records = object
            .get_mut(section)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                StoreError::Integrity(format!(
                    "scenario domain section {section} is not an object"
                ))
            })?;
        let taken = std::mem::take(records);
        for (key, mut value) in taken {
            let old = Uuid::parse_str(&key).map_err(|_| {
                StoreError::Integrity(format!("definition key {key} is not a UUID"))
            })?;
            let new = mapping.get(&old).ok_or_else(|| {
                StoreError::Integrity(format!("definition key {key} was not mapped"))
            })?;
            if let Some(record_id) = value.get_mut("id")
                && record_id.as_str() == Some(key.as_str())
            {
                *record_id = Value::String(new.to_string());
            }
            if records.insert(new.to_string(), value).is_some() {
                return Err(StoreError::Integrity(
                    "duplicate identity produced while copying scenario".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

fn rewrite_self_declared_definitions(
    value: &mut Value,
    mapping: &BTreeMap<Uuid, Uuid>,
) -> Result<(), StoreError> {
    match value {
        Value::Array(values) => {
            for value in values {
                rewrite_self_declared_definitions(value, mapping)?;
            }
        }
        Value::Object(values) => {
            let taken = std::mem::take(values);
            for (key, mut value) in taken {
                rewrite_self_declared_definitions(&mut value, mapping)?;
                let rewritten_key = Uuid::parse_str(&key)
                    .ok()
                    .and_then(|old| mapping.get(&old))
                    .filter(|_| value.get("id").and_then(Value::as_str) == Some(key.as_str()))
                    .map_or_else(
                        || key.clone(),
                        |new| {
                            if let Some(id) = value.get_mut("id") {
                                *id = Value::String(new.to_string());
                            }
                            new.to_string()
                        },
                    );
                if values.insert(rewritten_key.clone(), value).is_some() {
                    return Err(StoreError::Integrity(format!(
                        "definition rewrite produced duplicate key {rewritten_key}"
                    )));
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum DeclaredReferenceKind {
    Scalar,
    List,
    External,
}

fn rewrite_declared_references(
    value: &mut Value,
    mapping: &BTreeMap<Uuid, Uuid>,
) -> Result<(), StoreError> {
    match value {
        Value::Array(values) => {
            for value in values {
                rewrite_declared_references(value, mapping)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                match declared_reference_kind(key) {
                    Some(DeclaredReferenceKind::Scalar) => {
                        rewrite_uuid_string(value, mapping, key)?;
                    }
                    Some(DeclaredReferenceKind::List) => {
                        let items = value.as_array_mut().ok_or_else(|| {
                            StoreError::Integrity(format!(
                                "declared internal reference list {key} must be an array"
                            ))
                        })?;
                        for item in items {
                            rewrite_uuid_string(item, mapping, key)?;
                        }
                    }
                    Some(DeclaredReferenceKind::External) => {}
                    None => rewrite_declared_references(value, mapping)?,
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn rewrite_uuid_string(
    value: &mut Value,
    mapping: &BTreeMap<Uuid, Uuid>,
    field: &str,
) -> Result<(), StoreError> {
    let text = value.as_str().ok_or_else(|| {
        StoreError::Integrity(format!(
            "declared internal reference {field} is not a string"
        ))
    })?;
    let old = Uuid::parse_str(text).map_err(|_| {
        StoreError::Integrity(format!("declared internal reference {field} is not a UUID"))
    })?;
    if let Some(new) = mapping.get(&old) {
        *value = Value::String(new.to_string());
    }
    Ok(())
}

fn declared_reference_kind(key: &str) -> Option<DeclaredReferenceKind> {
    let normalized = key
        .chars()
        .filter(|character| !matches!(character, '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if matches!(
        normalized.as_str(),
        "externalid"
            | "externalids"
            | "providerexternalid"
            | "providerexternalids"
            | "sourceexternalid"
            | "sourceexternalids"
            | "legacyexternalid"
            | "legacyexternalids"
    ) {
        return Some(DeclaredReferenceKind::External);
    }
    match reference_field_tokens(key).last().map(String::as_str) {
        Some("id") => Some(DeclaredReferenceKind::Scalar),
        Some("ids") => Some(DeclaredReferenceKind::List),
        _ => None,
    }
}

fn reference_field_tokens(field: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut previous = None;
    let mut characters = field.chars().peekable();
    while let Some(character) = characters.next() {
        if !character.is_ascii_alphanumeric() {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
            previous = None;
            continue;
        }
        let boundary = !token.is_empty()
            && character.is_ascii_uppercase()
            && previous.is_some_and(|previous: char| {
                previous.is_ascii_lowercase()
                    || previous.is_ascii_digit()
                    || (previous.is_ascii_uppercase()
                        && characters.peek().is_some_and(char::is_ascii_lowercase))
            });
        if boundary {
            tokens.push(std::mem::take(&mut token));
        }
        token.push(character.to_ascii_lowercase());
        previous = Some(character);
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

fn reject_stale_declared_references(
    value: &Value,
    mapping: &BTreeMap<Uuid, Uuid>,
) -> Result<(), StoreError> {
    match value {
        Value::Array(values) => {
            for value in values {
                reject_stale_declared_references(value, mapping)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                match declared_reference_kind(key) {
                    Some(DeclaredReferenceKind::Scalar) => {
                        reject_stale_uuid(value, mapping, key)?;
                    }
                    Some(DeclaredReferenceKind::List) => {
                        let items = value.as_array().ok_or_else(|| {
                            StoreError::Integrity(format!(
                                "declared internal reference list {key} must be an array"
                            ))
                        })?;
                        for item in items {
                            reject_stale_uuid(item, mapping, key)?;
                        }
                    }
                    Some(DeclaredReferenceKind::External) => {}
                    None => reject_stale_declared_references(value, mapping)?,
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn reject_stale_uuid(
    value: &Value,
    mapping: &BTreeMap<Uuid, Uuid>,
    field: &str,
) -> Result<(), StoreError> {
    let text = value.as_str().ok_or_else(|| {
        StoreError::Integrity(format!(
            "declared internal reference {field} is not a string"
        ))
    })?;
    let id = Uuid::parse_str(text).map_err(|_| {
        StoreError::Integrity(format!("declared internal reference {field} is not a UUID"))
    })?;
    if mapping.contains_key(&id) {
        return Err(StoreError::Integrity(format!(
            "declared internal reference {field} retained an old identity"
        )));
    }
    Ok(())
}

fn ensure_revision(expected: Revision, actual: u64) -> Result<(), StoreError> {
    let actual = checked_revision(actual)?;
    if expected != actual {
        return Err(StoreError::Conflict { expected, actual });
    }
    Ok(())
}

fn validate_stored_portable_json(value: &Value, context: &str) -> Result<(), StoreError> {
    validate_nonsecret_portable_json(
        value,
        &PortableJsonLimits {
            max_depth: eutheto_export::PORTABLE_LIMITS.max_json_depth,
            max_string_bytes: eutheto_export::PORTABLE_LIMITS.max_string_bytes,
            max_collection_items: eutheto_export::PORTABLE_LIMITS.max_collection_items,
        },
    )
    .map_err(|error| {
        StoreError::Integrity(format!(
            "stored {context} violates the nonsecret portable policy: {error}"
        ))
    })
}

fn parse_timestamp(value: &str, field: &str) -> Result<Rfc3339Timestamp, StoreError> {
    Rfc3339Timestamp::parse(value)
        .map_err(|error| StoreError::Integrity(format!("invalid stored {field}: {error}")))
}

fn parse_optional_timestamp(
    value: Option<String>,
    field: &str,
) -> Result<Option<Rfc3339Timestamp>, StoreError> {
    value
        .map(|timestamp| parse_timestamp(&timestamp, field))
        .transpose()
}
fn validate_failure_receipt_fields(
    proof: &str,
    collision_plan_sha256: &str,
    safe_reason: &str,
) -> Result<(), StoreError> {
    if proof.trim().is_empty() || proof.len() > MAX_RECEIPT_PROOF_BYTES {
        return Err(StoreError::InvalidSafetyBackupFailureReceipt(
            "proof is empty or exceeds the receipt size limit".to_owned(),
        ));
    }
    if collision_plan_sha256 == proof || safe_reason.contains(proof) {
        return Err(StoreError::InvalidSafetyBackupFailureReceipt(
            "receipt metadata contains the plaintext proof".to_owned(),
        ));
    }
    if collision_plan_sha256.len() != 64
        || !collision_plan_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(StoreError::InvalidSafetyBackupFailureReceipt(
            "collision-plan digest is not SHA-256".to_owned(),
        ));
    }
    if safe_reason.trim().is_empty() || safe_reason.len() > MAX_RECEIPT_SAFE_REASON_BYTES {
        return Err(StoreError::InvalidSafetyBackupFailureReceipt(
            "safe reason is empty or exceeds the receipt size limit".to_owned(),
        ));
    }
    Ok(())
}
fn authorize_staged_restore(
    transaction: &rusqlite::Transaction<'_>,
    binding: &PreviewBinding,
    mode: RestoreMode,
    authorization: Option<&RestoreAuthorization>,
) -> Result<(), StoreError> {
    match mode {
        RestoreMode::ImportScenario => {
            if authorization.is_some() {
                return Err(StoreError::InvalidStagedApply(
                    "ordinary import carries restore authorization".to_owned(),
                ));
            }
        }
        RestoreMode::AddBackup => {
            let authorization = authorization.ok_or_else(|| {
                StoreError::InvalidStagedApply(
                    "add-backup restore lacks authorization metadata".to_owned(),
                )
            })?;
            if authorization.prospective_failure_receipt_token.is_some()
                || authorization.collision_plan_sha256.is_some()
            {
                return Err(StoreError::InvalidStagedApply(
                    "add-backup restore carries failure-receipt metadata".to_owned(),
                ));
            }
        }
        RestoreMode::ReplaceLibrary => {
            let authorization = authorization.ok_or_else(|| {
                StoreError::InvalidStagedApply(
                    "replace-library restore lacks authorization metadata".to_owned(),
                )
            })?;
            if authorization.prospective_failure_receipt_token.is_some() {
                return Err(StoreError::InvalidStagedApply(
                    "replace-library restore carries an unnormalized receipt token".to_owned(),
                ));
            }
            if let SafetyBackupEvidence::FailedWithStrongConfirmation { proof } =
                &authorization.safety_backup
            {
                let collision_plan_sha256 = authorization
                    .collision_plan_sha256
                    .as_deref()
                    .ok_or(StoreError::SafetyBackupFailureReceiptRejected)?;
                if proof.trim().is_empty()
                    || proof.len() > MAX_RECEIPT_PROOF_BYTES
                    || collision_plan_sha256.len() != 64
                    || !collision_plan_sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(StoreError::SafetyBackupFailureReceiptRejected);
                }
                let proof_sha256 = eutheto_export::sha256_hex(proof.as_bytes());
                let binding_json = eutheto_export::canonical_json(binding)
                    .map_err(|_| StoreError::SafetyBackupFailureReceiptRejected)?;
                let consumed = transaction.execute(
                    "DELETE FROM safety_backup_failure_receipts WHERE proof_sha256 = ?1 AND binding_json = ?2 AND collision_plan_sha256 = ?3",
                    params![proof_sha256, binding_json, collision_plan_sha256],
                )?;
                if consumed != 1 {
                    return Err(StoreError::SafetyBackupFailureReceiptRejected);
                }
            }
        }
    }
    Ok(())
}

fn command_source_name(source: CommandSource) -> &'static str {
    match source {
        CommandSource::Desktop => "desktop",
        CommandSource::Cli => "cli",
        CommandSource::Import => "import",
        CommandSource::System => "system",
        CommandSource::Undo => "undo",
        CommandSource::Redo => "redo",
    }
}

fn parse_command_source(value: &str) -> Result<CommandSource, StoreError> {
    serde_json::from_value(Value::String(value.to_owned()))
        .map_err(|error| StoreError::Integrity(format!("invalid stored command source: {error}")))
}

fn authorize_staged_apply(
    transaction: &rusqlite::Transaction<'_>,
    binding: &PreviewBinding,
    mode: RestoreMode,
    authorization: Option<&RestoreAuthorization>,
) -> Result<Revision, StoreError> {
    let actual_revision = library_revision(transaction)?;
    let expected_revision = binding.local_library_revision;
    if expected_revision != actual_revision {
        return Err(StoreError::LibraryConflict {
            expected: expected_revision,
            actual: actual_revision,
        });
    }
    authorize_staged_restore(transaction, binding, mode, authorization)?;
    Ok(actual_revision)
}

fn apply_staged_library_transaction(
    connection: &mut Connection,
    staged: StagedLibraryApply,
    applied_at: Rfc3339Timestamp,
    #[cfg(debug_assertions)] failpoint: &Arc<std::sync::Mutex<Option<Failpoint>>>,
) -> Result<LibraryApplyOutcome, StoreError> {
    let StagedApplyParts {
        import,
        remove_scenario_ids,
        settings,
        authorization,
    } = staged_apply_parts(staged)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let actual_revision = authorize_staged_apply(
        &transaction,
        &import.binding,
        import.mode,
        authorization.as_ref(),
    )?;

    let StagedImport {
        binding,
        mode,
        scenarios,
        scenario_revisions,
        results,
        shared_records,
        preferences,
        manifest_extensions,
        nonsemantic_extensions,
        assets,
        supplemental_replacements,
        provenance,
    } = import;
    ensure_supplemental_replacements(
        &transaction,
        mode,
        &results,
        &shared_records,
        &preferences,
        &assets,
        &supplemental_replacements,
    )?;
    let no_effect = mode != RestoreMode::ReplaceLibrary
        && remove_scenario_ids.is_empty()
        && scenarios.is_empty()
        && results.is_empty()
        && shared_records.is_empty()
        && preferences.is_empty()
        && assets.is_empty()
        && settings.is_empty();
    if no_effect {
        return Ok(LibraryApplyOutcome {
            library_revision: actual_revision,
            created: 0,
            replaced: 0,
            removed: 0,
        });
    }

    let (outcome, retained_candidates) = replace_staged_scenarios(
        &transaction,
        mode,
        scenarios,
        scenario_revisions,
        &remove_scenario_ids,
    )?;
    upsert_portable_section(&transaction, SupplementalSectionKind::Results, results)?;
    upsert_portable_section(
        &transaction,
        SupplementalSectionKind::SharedRecords,
        shared_records,
    )?;
    upsert_portable_section(
        &transaction,
        SupplementalSectionKind::Preferences,
        preferences,
    )?;
    upsert_portable_assets(&transaction, assets)?;
    synchronize_retained_scenario_revisions(&transaction, retained_candidates)?;
    upsert_settings(&transaction, settings)?;
    persist_portable_library_metadata(
        &transaction,
        mode,
        manifest_extensions,
        nonsemantic_extensions,
    )?;
    validate_global_identity_ownership(&transaction)?;
    #[cfg(debug_assertions)]
    consume_failpoint(failpoint, Failpoint::AfterSupplementalWrite)?;
    insert_import_provenance(
        &transaction,
        binding,
        provenance,
        &outcome.sources,
        applied_at,
    )?;
    let library_revision = increment_library_revision(&transaction)?;
    transaction.commit()?;
    Ok(LibraryApplyOutcome {
        library_revision,

        created: outcome.created,
        replaced: outcome.replaced,
        removed: remove_scenario_ids.len(),
    })
}
fn validate_staged_scenario_revision_targets(
    connection: &Connection,
    scenarios: &[eutheto_import::StagedScenario],
    staged_history: &[PortableScenario],
) -> Result<(), StoreError> {
    let mut high_water = load_scenario_revision_high_water(connection)?;
    for historical in staged_history {
        validate_scenario_owned_uuid_uniqueness(historical)
            .map_err(|error| StoreError::InvalidStagedApply(error.to_string()))?;
        record_scenario_identity_owners(
            connection,
            historical.document.scenario_id,
            &collect_scenario_owned_uuids(historical),
        )?;
        high_water
            .entry(historical.document.scenario_id)
            .and_modify(|revision| *revision = (*revision).max(historical.revision))
            .or_insert(historical.revision);
    }
    for staged in scenarios {
        validate_scenario_owned_uuid_uniqueness(&staged.scenario)
            .map_err(|error| StoreError::InvalidStagedApply(error.to_string()))?;
        let scenario_id = staged.scenario.document.scenario_id;
        if let Some(highest_revision) = high_water.get(&scenario_id)
            && staged.scenario.revision <= *highest_revision
        {
            return Err(StoreError::InvalidStagedApply(format!(
                "staged scenario {scenario_id} revision does not exceed its durable high-water mark"
            )));
        }
    }
    Ok(())
}

fn replace_staged_scenarios(
    transaction: &rusqlite::Transaction<'_>,
    mode: RestoreMode,
    scenarios: Vec<eutheto_import::StagedScenario>,
    mut retained_candidates: Vec<PortableScenario>,
    remove_scenario_ids: &BTreeSet<ScenarioId>,
) -> Result<(StagedScenarioOutcome, Vec<PortableScenario>), StoreError> {
    let current_revisions = scenario_revisions(transaction)?;
    let current_ids = current_revisions.keys().copied().collect::<BTreeSet<_>>();
    if mode == RestoreMode::ReplaceLibrary && remove_scenario_ids != &current_ids {
        return Err(StoreError::InvalidStagedApply(
            "replace-library removal set no longer matches the library".to_owned(),
        ));
    }
    validate_staged_scenario_revision_targets(transaction, &scenarios, &retained_candidates)?;
    if mode != RestoreMode::ReplaceLibrary {
        retained_candidates
            .extend(read_retained_scenario_revision_rows(transaction)?.into_values());
    }
    retained_candidates.extend(scenarios.iter().map(|staged| {
        let mut scenario = staged.scenario.clone();
        scenario.project = None;
        scenario
    }));
    if mode == RestoreMode::ReplaceLibrary {
        transaction.execute("DELETE FROM portable_sections", [])?;
        transaction.execute("DELETE FROM portable_import_provenance", [])?;
        transaction.execute("DELETE FROM app_settings", [])?;
        transaction.execute("DELETE FROM retained_scenario_revisions", [])?;
    }
    for (scenario_id, revision) in &current_revisions {
        record_scenario_revision_high_water(transaction, *scenario_id, *revision)?;
    }
    for scenario_id in remove_scenario_ids {
        transaction.execute(
            "DELETE FROM scenarios WHERE id = ?1",
            [scenario_id.to_string()],
        )?;
    }
    let mut occupied = current_ids;
    for scenario_id in remove_scenario_ids {
        occupied.remove(scenario_id);
    }
    let outcome =
        apply_staged_scenarios(transaction, mode, scenarios, remove_scenario_ids, occupied)?;
    Ok((outcome, retained_candidates))
}

fn apply_staged_scenarios(
    transaction: &rusqlite::Transaction<'_>,
    mode: RestoreMode,
    scenarios: Vec<eutheto_import::StagedScenario>,
    remove_scenario_ids: &BTreeSet<ScenarioId>,
    mut occupied: BTreeSet<ScenarioId>,
) -> Result<StagedScenarioOutcome, StoreError> {
    let mut created = 0;
    let mut replaced = 0;
    let mut staged_ids = BTreeSet::new();
    let mut sources = Vec::with_capacity(scenarios.len());
    for staged_scenario in scenarios {
        let source_revision = staged_scenario.source_revision;
        let target_revision = staged_scenario.scenario.revision;
        let document = staged_scenario.scenario.document;
        let scenario_id = document.scenario_id;
        if !staged_ids.insert(scenario_id) {
            return Err(StoreError::InvalidStagedApply(
                "staged input contains a duplicate scenario identity".to_owned(),
            ));
        }
        sources.push(ScenarioImportSource {
            scenario_id,
            original_scenario_id: staged_scenario.original_id,
            source_revision,
            disposition: persisted_disposition(staged_scenario.disposition),
            id_remap: staged_scenario.id_remap,
        });
        let archived_at = if mode == RestoreMode::ImportScenario {
            if staged_scenario.disposition == StagedDisposition::Replace {
                scenario_archived_at(transaction, scenario_id)?
            } else {
                None
            }
        } else {
            staged_scenario
                .scenario
                .project
                .as_ref()
                .and_then(|project| project.archived_at)
        };
        let portable = PortableWrapperMetadata {
            required_capabilities: staged_scenario.scenario.required_capabilities,
            semantic_extensions: staged_scenario.scenario.semantic_extensions,
            extensions: staged_scenario.scenario.extensions,
        };
        match staged_scenario.disposition {
            StagedDisposition::Create | StagedDisposition::CreateCopy => {
                if !occupied.insert(scenario_id) {
                    return Err(StoreError::ScenarioAlreadyExists(scenario_id));
                }
                insert_project(
                    transaction,
                    &document,
                    target_revision,
                    archived_at,
                    &portable,
                )?;
                created += 1;
            }
            StagedDisposition::Replace => {
                let revision = target_revision;
                if remove_scenario_ids.contains(&scenario_id) {
                    if !occupied.insert(scenario_id) {
                        return Err(StoreError::InvalidStagedApply(
                            "staged input contains a duplicate scenario identity".to_owned(),
                        ));
                    }
                } else if occupied.contains(&scenario_id) {
                    transaction.execute(
                        "DELETE FROM scenarios WHERE id = ?1",
                        [scenario_id.to_string()],
                    )?;
                } else {
                    return Err(StoreError::ScenarioNotFound(scenario_id));
                }
                insert_project(transaction, &document, revision, archived_at, &portable)?;
                replaced += 1;
            }
        }
    }
    sources.sort_by_key(|source| source.scenario_id);
    Ok(StagedScenarioOutcome {
        created,
        replaced,
        sources,
    })
}

fn staged_apply_parts(staged: StagedLibraryApply) -> Result<StagedApplyParts, StoreError> {
    match staged {
        StagedLibraryApply::Import(import) => {
            if import.mode != RestoreMode::ImportScenario {
                return Err(StoreError::InvalidStagedApply(
                    "ordinary import must use import-scenario mode".to_owned(),
                ));
            }
            Ok(StagedApplyParts {
                import,
                remove_scenario_ids: BTreeSet::new(),
                settings: BTreeMap::new(),
                authorization: None,
            })
        }
        StagedLibraryApply::BackupRestore { restore, settings } => {
            let StagedBackupRestore {
                import,
                remove_scenario_ids,
                authorization,
            } = restore;
            match import.mode {
                RestoreMode::ImportScenario => Err(StoreError::InvalidStagedApply(
                    "backup restore cannot use import-scenario mode".to_owned(),
                )),
                RestoreMode::AddBackup => {
                    if !remove_scenario_ids.is_empty()
                        || authorization.safety_backup != SafetyBackupEvidence::NotRequired
                    {
                        return Err(StoreError::InvalidStagedApply(
                            "add-backup restore has destructive metadata".to_owned(),
                        ));
                    }
                    Ok(StagedApplyParts {
                        import,
                        remove_scenario_ids,
                        settings,
                        authorization: Some(authorization),
                    })
                }
                RestoreMode::ReplaceLibrary => {
                    if !authorization.destructive_action_confirmed
                        || authorization.safety_backup == SafetyBackupEvidence::NotRequired
                    {
                        return Err(StoreError::InvalidStagedApply(
                            "replace-library restore lacks destructive authorization".to_owned(),
                        ));
                    }
                    Ok(StagedApplyParts {
                        import,
                        remove_scenario_ids,
                        settings,
                        authorization: Some(authorization),
                    })
                }
            }
        }
    }
}

fn scenario_revisions(
    connection: &Connection,
) -> Result<BTreeMap<ScenarioId, Revision>, StoreError> {
    let mut statement = connection.prepare("SELECT id, revision FROM scenarios ORDER BY id")?;
    let mut rows = statement.query([])?;
    let mut revisions = BTreeMap::new();
    while let Some(row) = rows.next()? {
        let value: String = row.get(0)?;
        let id = value.parse().map_err(|error| {
            StoreError::Integrity(format!("invalid stored scenario id: {error}"))
        })?;
        let revision = checked_revision(i64_to_u64(row.get(1)?)?)?;
        revisions.insert(id, revision);
    }
    Ok(revisions)
}

fn persisted_disposition(disposition: StagedDisposition) -> PersistedDisposition {
    match disposition {
        StagedDisposition::Create => PersistedDisposition::Create,
        StagedDisposition::CreateCopy => PersistedDisposition::CreateCopy,
        StagedDisposition::Replace => PersistedDisposition::Replace,
    }
}

fn persisted_migration(migration: AppliedMigration) -> PersistedAppliedMigration {
    let registry = match migration.registry {
        MigrationRegistryKind::Outer => PersistedMigrationRegistry::Outer,
        MigrationRegistryKind::Portable => PersistedMigrationRegistry::Portable,
    };
    PersistedAppliedMigration {
        registry,
        name: migration.name,
        from_version: migration.from_version,
        to_version: migration.to_version,
    }
}

fn delete_scenario_referencing_supplemental(
    transaction: &rusqlite::Transaction<'_>,
    scenario_id: ScenarioId,
) -> Result<(), StoreError> {
    let mut statement = transaction.prepare(
        "SELECT section, key, value FROM portable_sections WHERE section <> ?1 ORDER BY section, key",
    )?;
    let mut rows = statement.query([SupplementalSectionKind::Assets.as_str()])?;
    let mut referencing_records = Vec::new();
    while let Some(row) = rows.next()? {
        let section: String = row.get(0)?;
        let key: String = row.get(1)?;
        let bytes: Vec<u8> = row.get(2)?;
        let value: Value = serde_json::from_slice(&bytes)?;
        let scenario_references = extract_scenario_references(&value).map_err(|error| {
            StoreError::Integrity(format!(
                "stored supplemental record has invalid scenario references: {error}"
            ))
        })?;
        if scenario_references.contains(&scenario_id) {
            referencing_records.push((section, key));
        }
    }
    drop(rows);
    drop(statement);
    let mut delete =
        transaction.prepare("DELETE FROM portable_sections WHERE section = ?1 AND key = ?2")?;
    for (section, key) in referencing_records {
        delete.execute(params![section, key])?;
    }
    Ok(())
}

fn ensure_supplemental_replacements(
    transaction: &rusqlite::Transaction<'_>,
    mode: RestoreMode,
    results: &BTreeMap<String, Vec<u8>>,
    shared_records: &BTreeMap<String, Vec<u8>>,
    preferences: &BTreeMap<String, Vec<u8>>,
    assets: &BTreeMap<String, PortableAsset>,
    authorized: &BTreeSet<SupplementalIdentity>,
) -> Result<(), StoreError> {
    if mode == RestoreMode::ReplaceLibrary {
        return Ok(());
    }
    let current = load_supplemental_identities(transaction)?;
    let staged = supplemental_identities_from_keys(results, shared_records, preferences, assets);
    let collisions = current
        .intersection(&staged)
        .cloned()
        .collect::<BTreeSet<_>>();
    if collisions != *authorized {
        return Err(StoreError::InvalidStagedApply(
            "supplemental replacement authorizations do not exactly match current collisions"
                .to_owned(),
        ));
    }
    Ok(())
}

fn supplemental_identities_from_keys(
    results: &BTreeMap<String, Vec<u8>>,
    shared_records: &BTreeMap<String, Vec<u8>>,
    preferences: &BTreeMap<String, Vec<u8>>,
    assets: &BTreeMap<String, PortableAsset>,
) -> BTreeSet<SupplementalIdentity> {
    let mut identities = BTreeSet::new();
    extend_supplemental_identities(
        &mut identities,
        SupplementalSectionKind::Results,
        results.keys(),
    );
    extend_supplemental_identities(
        &mut identities,
        SupplementalSectionKind::SharedRecords,
        shared_records.keys(),
    );
    extend_supplemental_identities(
        &mut identities,
        SupplementalSectionKind::Preferences,
        preferences.keys(),
    );
    extend_supplemental_identities(
        &mut identities,
        SupplementalSectionKind::Assets,
        assets.keys(),
    );
    identities
}

fn portable_section_identities(
    sections: &PortableSupplementalSections,
) -> BTreeSet<SupplementalIdentity> {
    supplemental_identities_from_keys(
        &sections.results,
        &sections.shared_records,
        &sections.preferences,
        &sections.assets,
    )
}

fn extend_supplemental_identities<'a>(
    identities: &mut BTreeSet<SupplementalIdentity>,
    section: SupplementalSectionKind,
    keys: impl Iterator<Item = &'a String>,
) {
    identities.extend(keys.map(|key| SupplementalIdentity {
        section,
        key: key.clone(),
    }));
}

fn load_supplemental_identities(
    connection: &Connection,
) -> Result<BTreeSet<SupplementalIdentity>, StoreError> {
    let mut statement =
        connection.prepare("SELECT section, key FROM portable_sections ORDER BY section, key")?;
    let mut rows = statement.query([])?;
    let mut identities = BTreeSet::new();
    while let Some(row) = rows.next()? {
        identities.insert(SupplementalIdentity {
            section: supplemental_section_kind(&row.get::<_, String>(0)?)?,
            key: row.get(1)?,
        });
    }
    Ok(identities)
}

fn load_supplemental_identity_owners(
    connection: &Connection,
) -> Result<BTreeMap<Uuid, SupplementalIdentity>, StoreError> {
    let mut statement = connection.prepare("SELECT section, key, value FROM portable_sections")?;
    let mut rows = statement.query([])?;
    let mut owners = BTreeMap::new();
    while let Some(row) = rows.next()? {
        let section = supplemental_section_kind(&row.get::<_, String>(0)?)?;
        let key: String = row.get(1)?;
        let bytes: Vec<u8> = row.get(2)?;
        let owner = SupplementalIdentity {
            section,
            key: key.clone(),
        };
        let value = if section == SupplementalSectionKind::Assets {
            None
        } else {
            Some(serde_json::from_slice::<Value>(&bytes)?)
        };
        let exact = if section == SupplementalSectionKind::Results {
            let result = value.as_ref().ok_or_else(|| {
                StoreError::Integrity("stored result is missing its JSON value".to_owned())
            })?;
            Some(extract_result_id(result).map_err(|error| {
                StoreError::Integrity(format!("stored result has invalid identity: {error}"))
            })?)
        } else {
            Uuid::parse_str(key.split('.').next().unwrap_or(&key)).ok()
        };
        if let Some(identity) = exact {
            register_supplemental_identity_owner(&mut owners, identity, &owner)?;
        }
        if let Some(value) = value {
            for identity in collect_self_declared_uuids(&value) {
                register_supplemental_identity_owner(&mut owners, identity, &owner)?;
            }
        }
    }
    Ok(owners)
}

fn register_supplemental_identity_owner(
    owners: &mut BTreeMap<Uuid, SupplementalIdentity>,
    identity: Uuid,
    owner: &SupplementalIdentity,
) -> Result<(), StoreError> {
    if let Some(existing) = owners.get(&identity) {
        if existing == owner {
            return Ok(());
        }
        return Err(StoreError::IdentityCollision(identity));
    }
    owners.insert(identity, owner.clone());
    Ok(())
}

fn supplemental_section_kind(section: &str) -> Result<SupplementalSectionKind, StoreError> {
    match section {
        "results" => Ok(SupplementalSectionKind::Results),
        "shared_records" => Ok(SupplementalSectionKind::SharedRecords),
        "preferences" => Ok(SupplementalSectionKind::Preferences),
        "assets" => Ok(SupplementalSectionKind::Assets),
        _ => Err(StoreError::Integrity(format!(
            "invalid stored portable section: {section}"
        ))),
    }
}

fn upsert_portable_section(
    transaction: &rusqlite::Transaction<'_>,
    section: SupplementalSectionKind,
    values: BTreeMap<String, Vec<u8>>,
) -> Result<(), StoreError> {
    let mut statement = transaction.prepare(
        "INSERT INTO portable_sections (section, key, value, asset_media_type, asset_redistribution_permitted) VALUES (?1, ?2, ?3, NULL, NULL) ON CONFLICT(section, key) DO UPDATE SET value = excluded.value, asset_media_type = NULL, asset_redistribution_permitted = NULL",
    )?;
    for (key, value) in values {
        let parsed: Value = serde_json::from_slice(&value)?;
        validate_stored_portable_json(&parsed, "staged supplemental portable record")?;
        statement.execute(params![section.as_str(), key, value])?;
    }
    Ok(())
}

fn upsert_portable_assets(
    transaction: &rusqlite::Transaction<'_>,
    assets: BTreeMap<String, PortableAsset>,
) -> Result<(), StoreError> {
    let mut statement = transaction.prepare(
        "INSERT INTO portable_sections (section, key, value, asset_media_type, asset_redistribution_permitted) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(section, key) DO UPDATE SET value = excluded.value, asset_media_type = excluded.asset_media_type, asset_redistribution_permitted = excluded.asset_redistribution_permitted",
    )?;
    for (key, asset) in assets {
        statement.execute(params![
            SupplementalSectionKind::Assets.as_str(),
            key,
            asset.bytes,
            asset.media_type,
            i64::from(asset.redistribution_permitted),
        ])?;
    }
    Ok(())
}

fn upsert_settings(
    transaction: &rusqlite::Transaction<'_>,
    settings: BTreeMap<String, AppSetting<Value>>,
) -> Result<(), StoreError> {
    let mut statement = transaction.prepare(
        "INSERT INTO app_settings (key, value_json, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
    )?;
    for (key, setting) in settings {
        validate_stored_portable_json(&setting.value, "staged application setting")?;
        statement.execute(params![
            key,
            serde_json::to_string(&setting.value)?,
            setting.updated_at.to_string(),
        ])?;
    }
    Ok(())
}

fn insert_import_provenance(
    transaction: &rusqlite::Transaction<'_>,
    binding: PreviewBinding,
    provenance: ImportProvenance,
    scenario_sources: &[ScenarioImportSource],
    applied_at: Rfc3339Timestamp,
) -> Result<(), StoreError> {
    let binding = PersistedPreviewBinding {
        file_sha256: binding.file_sha256,
        options_sha256: binding.options_sha256,
        local_library_revision: binding.local_library_revision,
        format_version: binding.format_version,
        schema_version: binding.schema_version,
    };
    let applied_migrations = provenance
        .applied_migrations
        .into_iter()
        .map(persisted_migration)
        .collect::<Vec<_>>();
    let source_bundle_id = provenance.source_bundle_id.to_string();
    let source_application = serde_json::to_string(&provenance.source_application)?;
    let applied_migrations = serde_json::to_string(&applied_migrations)?;
    let binding = serde_json::to_string(&binding)?;
    let scenario_sources = serde_json::to_string(scenario_sources)?;
    let source_created_at = provenance.source_created_at.to_string();
    let applied_at = applied_at.to_string();
    let row_bytes = [
        source_bundle_id.len(),
        source_application.len(),
        provenance.source_file_sha256.len(),
        applied_migrations.len(),
        binding.len(),
        scenario_sources.len(),
        source_created_at.len(),
        applied_at.len(),
    ]
    .into_iter()
    .try_fold(16_u64, |total, size| {
        total
            .checked_add(u64::try_from(size).map_err(|_| StoreError::NumericRange)?)
            .ok_or(StoreError::NumericRange)
    })?;
    if row_bytes > MAX_IMPORT_PROVENANCE_BYTES {
        return Err(StoreError::InvalidStagedApply(
            "portable import provenance exceeds the retention byte policy".to_owned(),
        ));
    }
    transaction.execute(
        "INSERT INTO portable_import_provenance (source_bundle_id, source_application_json, original_format_version, original_schema_version, source_file_sha256, applied_migrations_json, binding_json, scenario_sources_json, source_created_at, applied_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            source_bundle_id,
            source_application,
            u32_to_i64(provenance.original_format_version),
            u32_to_i64(provenance.original_schema_version),
            provenance.source_file_sha256,
            applied_migrations,
            binding,
            scenario_sources,
            source_created_at,
            applied_at,
        ],
    )?;
    prune_import_provenance(transaction)
}

fn prune_import_provenance(connection: &Connection) -> Result<(), StoreError> {
    let mut statement = connection.prepare(
        "SELECT id, 16 + length(CAST(source_bundle_id AS BLOB)) + length(CAST(source_application_json AS BLOB)) + length(CAST(source_file_sha256 AS BLOB)) + length(CAST(applied_migrations_json AS BLOB)) + length(CAST(binding_json AS BLOB)) + length(CAST(scenario_sources_json AS BLOB)) + length(CAST(source_created_at AS BLOB)) + length(CAST(applied_at AS BLOB)) FROM portable_import_provenance ORDER BY id",
    )?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let mut total_bytes = rows.iter().try_fold(0_u64, |total, (_, bytes)| {
        total
            .checked_add(i64_to_u64(*bytes)?)
            .ok_or(StoreError::NumericRange)
    })?;
    let mut retained_count = u64::try_from(rows.len()).map_err(|_| StoreError::NumericRange)?;
    for (id, bytes) in rows {
        if retained_count <= MAX_IMPORT_PROVENANCE_ROWS
            && total_bytes <= MAX_IMPORT_PROVENANCE_BYTES
        {
            break;
        }
        connection.execute("DELETE FROM portable_import_provenance WHERE id = ?1", [id])?;
        retained_count = retained_count
            .checked_sub(1)
            .ok_or(StoreError::NumericRange)?;
        total_bytes = total_bytes
            .checked_sub(i64_to_u64(bytes)?)
            .ok_or(StoreError::NumericRange)?;
    }
    Ok(())
}

fn load_portable_library_metadata(
    connection: &Connection,
) -> Result<(BTreeMap<String, Value>, BTreeSet<String>), StoreError> {
    let row = connection
        .query_row(
            "SELECT manifest_extensions_json, nonsemantic_extensions_json FROM portable_library_metadata WHERE singleton = 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or_else(|| {
            StoreError::Integrity("portable library metadata is missing".to_owned())
        })?;
    let manifest_extensions: Value = serde_json::from_str(&row.0)?;
    validate_stored_portable_json(&manifest_extensions, "manifest extensions")?;
    Ok((
        serde_json::from_value(manifest_extensions)?,
        serde_json::from_str(&row.1)?,
    ))
}

fn persist_portable_library_metadata(
    connection: &Connection,
    mode: RestoreMode,
    incoming_manifest_extensions: BTreeMap<String, Value>,
    incoming_nonsemantic_extensions: BTreeSet<String>,
) -> Result<(), StoreError> {
    let (mut manifest_extensions, mut nonsemantic_extensions) =
        if mode == RestoreMode::ReplaceLibrary {
            (BTreeMap::new(), BTreeSet::new())
        } else {
            load_portable_library_metadata(connection)?
        };
    manifest_extensions.extend(incoming_manifest_extensions);
    nonsemantic_extensions.extend(incoming_nonsemantic_extensions);
    let manifest_value = serde_json::to_value(&manifest_extensions)?;
    validate_stored_portable_json(&manifest_value, "staged manifest extensions")?;
    connection.execute(
        "UPDATE portable_library_metadata SET manifest_extensions_json = ?1, nonsemantic_extensions_json = ?2 WHERE singleton = 1",
        params![
            serde_json::to_string(&manifest_value)?,
            serde_json::to_string(&nonsemantic_extensions)?
        ],
    )?;
    Ok(())
}
fn load_all_settings(
    connection: &Connection,
) -> Result<BTreeMap<String, AppSetting<Value>>, StoreError> {
    let mut statement =
        connection.prepare("SELECT key, value_json, updated_at FROM app_settings ORDER BY key")?;
    let mut rows = statement.query([])?;
    let mut settings = BTreeMap::new();
    while let Some(row) = rows.next()? {
        let key: String = row.get(0)?;
        let json: String = row.get(1)?;
        let value: Value = serde_json::from_str(&json)?;
        validate_stored_portable_json(&value, "application setting")?;
        let updated_at: String = row.get(2)?;
        settings.insert(
            key,
            AppSetting {
                value,
                updated_at: parse_timestamp(&updated_at, "app setting updated_at")?,
            },
        );
    }
    Ok(settings)
}

fn load_portable_sections(
    connection: &Connection,
) -> Result<PortableSupplementalSections, StoreError> {
    let mut statement = connection.prepare(
        "SELECT section, key, value, asset_media_type, asset_redistribution_permitted FROM portable_sections ORDER BY section, key",
    )?;
    let mut rows = statement.query([])?;
    let mut sections = PortableSupplementalSections::default();
    while let Some(row) = rows.next()? {
        let raw_section: String = row.get(0)?;
        let section = supplemental_section_kind(&raw_section)?;
        let key: String = row.get(1)?;
        let value: Vec<u8> = row.get(2)?;
        let media_type: Option<String> = row.get(3)?;
        let redistribution_permitted: Option<i64> = row.get(4)?;
        if section != SupplementalSectionKind::Assets {
            let parsed: Value = serde_json::from_slice(&value)?;
            validate_stored_portable_json(&parsed, "supplemental portable record")?;
        }
        match section {
            SupplementalSectionKind::Results => {
                ensure_absent_asset_metadata(
                    &raw_section,
                    media_type.as_deref(),
                    redistribution_permitted,
                )?;
                sections.results.insert(key, value);
            }
            SupplementalSectionKind::SharedRecords => {
                ensure_absent_asset_metadata(
                    &raw_section,
                    media_type.as_deref(),
                    redistribution_permitted,
                )?;
                sections.shared_records.insert(key, value);
            }
            SupplementalSectionKind::Preferences => {
                ensure_absent_asset_metadata(
                    &raw_section,
                    media_type.as_deref(),
                    redistribution_permitted,
                )?;
                sections.preferences.insert(key, value);
            }
            SupplementalSectionKind::Assets => {
                let media_type = media_type.ok_or_else(|| {
                    StoreError::Integrity("stored asset is missing its media type".to_owned())
                })?;
                let redistribution_permitted = match redistribution_permitted {
                    Some(0) => false,
                    Some(1) => true,
                    _ => {
                        return Err(StoreError::Integrity(
                            "stored asset has invalid redistribution permission".to_owned(),
                        ));
                    }
                };
                sections.assets.insert(
                    key,
                    PortableAsset {
                        bytes: value,
                        media_type,
                        redistribution_permitted,
                    },
                );
            }
        }
    }
    Ok(sections)
}

fn ensure_absent_asset_metadata(
    section: &str,
    media_type: Option<&str>,
    redistribution_permitted: Option<i64>,
) -> Result<(), StoreError> {
    if media_type.is_some() || redistribution_permitted.is_some() {
        return Err(StoreError::Integrity(format!(
            "stored {section} record has asset metadata"
        )));
    }
    Ok(())
}

fn load_import_provenance(
    connection: &Connection,
) -> Result<Vec<PersistedImportProvenance>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT source_bundle_id, source_application_json, original_format_version, original_schema_version, source_file_sha256, applied_migrations_json, binding_json, scenario_sources_json, source_created_at, applied_at FROM portable_import_provenance ORDER BY id",
    )?;
    let mut rows = statement.query([])?;
    let mut all = Vec::new();
    while let Some(row) = rows.next()? {
        let source_bundle_id: String = row.get(0)?;
        let source_bundle_id = source_bundle_id.parse().map_err(|error| {
            StoreError::Integrity(format!("invalid stored source bundle id: {error}"))
        })?;
        let source_application_json: String = row.get(1)?;
        let original_format_version = i64_to_u32(row.get(2)?)?;
        let original_schema_version = i64_to_u32(row.get(3)?)?;
        let source_file_sha256: String = row.get(4)?;
        let applied_migrations_json: String = row.get(5)?;
        let binding_json: String = row.get(6)?;
        let scenario_sources_json: String = row.get(7)?;
        let source_created_at: String = row.get(8)?;
        let applied_at: String = row.get(9)?;
        all.push(PersistedImportProvenance {
            source_bundle_id,
            source_application: serde_json::from_str(&source_application_json)?,
            original_format_version,
            original_schema_version,
            source_file_sha256,
            applied_migrations: serde_json::from_str(&applied_migrations_json)?,
            scenario_sources: serde_json::from_str(&scenario_sources_json)?,
            binding: serde_json::from_str(&binding_json)?,
            source_created_at: parse_timestamp(
                &source_created_at,
                "import provenance source_created_at",
            )?,
            applied_at: parse_timestamp(&applied_at, "import provenance applied_at")?,
        });
    }
    Ok(all)
}

fn library_revision(connection: &Connection) -> Result<Revision, StoreError> {
    let value = connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key = ?1",
            [LIBRARY_REVISION_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_else(|| "0".to_owned());
    let parsed = value.parse().map_err(|error| {
        StoreError::Integrity(format!("invalid portable-library revision: {error}"))
    })?;
    Revision::try_new(parsed).map_err(|_| {
        StoreError::Integrity("portable-library revision exceeds the version-1 bound".to_owned())
    })
}

fn load_scenario_revision_high_water(
    connection: &Connection,
) -> Result<BTreeMap<ScenarioId, Revision>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT scenario_id, highest_revision FROM scenario_revision_high_water ORDER BY scenario_id",
    )?;
    let mut rows = statement.query([])?;
    let mut high_water = BTreeMap::new();
    while let Some(row) = rows.next()? {
        let scenario_id: String = row.get(0)?;
        let scenario_id = scenario_id.parse().map_err(|error| {
            StoreError::Integrity(format!(
                "invalid scenario revision high-water identity: {error}"
            ))
        })?;
        let revision = checked_revision(i64_to_u64(row.get(1)?)?)?;
        high_water.insert(scenario_id, revision);
    }
    Ok(high_water)
}

fn record_scenario_revision_high_water(
    connection: &Connection,
    scenario_id: ScenarioId,
    revision: Revision,
) -> Result<(), StoreError> {
    connection.execute(
        "INSERT INTO scenario_revision_high_water (scenario_id, highest_revision) VALUES (?1, ?2) ON CONFLICT(scenario_id) DO UPDATE SET highest_revision = excluded.highest_revision WHERE excluded.highest_revision > scenario_revision_high_water.highest_revision",
        params![scenario_id.to_string(), u64_to_i64(revision.value())?],
    )?;
    Ok(())
}

fn load_scenario_identity_owners(
    connection: &Connection,
) -> Result<BTreeMap<Uuid, ScenarioId>, StoreError> {
    let mut statement = connection
        .prepare("SELECT identity, scenario_id FROM scenario_identity_owners ORDER BY identity")?;
    let mut rows = statement.query([])?;
    let mut owners = BTreeMap::new();
    while let Some(row) = rows.next()? {
        let identity: String = row.get(0)?;
        let identity = Uuid::parse_str(&identity).map_err(|error| {
            StoreError::Integrity(format!("invalid reserved portable identity: {error}"))
        })?;
        let scenario_id: String = row.get(1)?;
        let scenario_id = scenario_id.parse().map_err(|error| {
            StoreError::Integrity(format!("invalid portable identity owner: {error}"))
        })?;
        if owners.insert(identity, scenario_id).is_some() {
            return Err(StoreError::Integrity(
                "reserved portable identity is duplicated".to_owned(),
            ));
        }
    }
    Ok(owners)
}

fn record_scenario_identity_owners(
    connection: &Connection,
    scenario_id: ScenarioId,
    identities: &BTreeSet<Uuid>,
) -> Result<(), StoreError> {
    for identity in identities {
        let existing: Option<String> = connection
            .query_row(
                "SELECT scenario_id FROM scenario_identity_owners WHERE identity = ?1",
                [identity.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            let existing: ScenarioId = existing.parse().map_err(|error| {
                StoreError::Integrity(format!("invalid portable identity owner: {error}"))
            })?;
            if existing != scenario_id {
                return Err(StoreError::IdentityCollision(*identity));
            }
            continue;
        }
        connection.execute(
            "INSERT INTO scenario_identity_owners (identity, scenario_id) VALUES (?1, ?2)",
            params![identity.to_string(), scenario_id.to_string()],
        )?;
    }
    Ok(())
}

fn ensure_scenario_id_unseen(
    connection: &Connection,
    scenario_id: ScenarioId,
) -> Result<(), StoreError> {
    let seen: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM scenario_revision_high_water WHERE scenario_id = ?1)",
        [scenario_id.to_string()],
        |row| row.get(0),
    )?;
    if seen {
        return Err(StoreError::ScenarioAlreadyExists(scenario_id));
    }
    Ok(())
}

fn increment_library_revision(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<Revision, StoreError> {
    let revision = library_revision(transaction)?
        .checked_next()
        .map_err(|_| StoreError::NumericRange)?;
    transaction.execute(
        "INSERT INTO app_metadata (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![LIBRARY_REVISION_KEY, revision.value().to_string()],
    )?;
    Ok(revision)
}

fn validate_project_owned_uuid_uniqueness(
    document: &ScenarioDocument,
    revision: Revision,
    portable: &PortableWrapperMetadata,
) -> Result<(), String> {
    let mut scenario = PortableScenario::current(
        revision,
        document.clone(),
        portable.required_capabilities.clone(),
    );
    scenario.semantic_extensions = portable.semantic_extensions.clone();
    scenario.extensions = portable.extensions.clone();
    validate_scenario_owned_uuid_uniqueness(&scenario).map_err(|error| error.to_string())
}
fn insert_project(
    transaction: &rusqlite::Transaction<'_>,
    document: &ScenarioDocument,
    revision: Revision,
    archived_at: Option<Rfc3339Timestamp>,
    portable: &PortableWrapperMetadata,
) -> Result<(), StoreError> {
    validate_project_owned_uuid_uniqueness(document, revision, portable)
        .map_err(StoreError::InvalidScenarioIdentity)?;
    record_scenario_identity_owners(
        transaction,
        document.scenario_id,
        &collect_project_owned_uuids(
            document,
            &portable.semantic_extensions,
            &portable.extensions,
        ),
    )?;
    validate_stored_portable_json(
        &serde_json::to_value(&portable.semantic_extensions)?,
        "scenario semantic extensions",
    )?;
    validate_stored_portable_json(
        &serde_json::to_value(&portable.extensions)?,
        "scenario extensions",
    )?;
    let revision_i64 = u64_to_i64(revision.value())?;
    let document_json = serialize_document(document)?;
    let required_capabilities_json = serde_json::to_string(&portable.required_capabilities)?;
    let semantic_extensions_json = serde_json::to_string(&portable.semantic_extensions)?;
    let extensions_json = serde_json::to_string(&portable.extensions)?;
    let projection = document_projection(document);
    let scenario_id = document.scenario_id;
    transaction.execute(
        "INSERT INTO scenarios (id, domain_pack_id, domain_schema_version, title, description, revision, document_json, portable_required_capabilities_json, portable_semantic_extensions_json, portable_extensions_json, created_at, updated_at, archived_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            scenario_id.to_string(),

            projection.domain_pack_id,
            u32_to_i64(projection.domain_schema_version),
            projection.title,
            projection.description,
            revision_i64,
            document_json,
            required_capabilities_json,
            semantic_extensions_json,
            extensions_json,
            projection.created_at,
            projection.updated_at,
            archived_at.map(|value| value.to_string()),
        ],
    )?;
    record_scenario_revision_high_water(transaction, scenario_id, revision)?;
    transaction.execute(
        "INSERT INTO scenario_history_state (scenario_id, cursor_sequence, branch_generation) VALUES (?1, 0, 0)",
        [scenario_id.to_string()],
    )?;
    Ok(())
}

fn serialize_document(document: &ScenarioDocument) -> Result<String, StoreError> {
    let json = serde_json::to_string(document)?;
    if u64::try_from(json.len()).map_err(|_| StoreError::NumericRange)?
        > MAX_SCENARIO_DOCUMENT_BYTES
    {
        return Err(StoreError::ScenarioDocumentTooLarge);
    }
    let value: Value = serde_json::from_str(&json)?;
    validate_stored_portable_json(&value, "scenario document")?;
    Ok(json)
}

fn u32_to_i64(value: u32) -> i64 {
    i64::from(value)
}

fn u64_to_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::NumericRange)
}

fn i64_to_u64(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::NumericRange)
}

fn i64_to_u32(value: i64) -> Result<u32, StoreError> {
    u32::try_from(value).map_err(|_| StoreError::NumericRange)
}

fn checked_revision(value: u64) -> Result<Revision, StoreError> {
    Revision::try_new(value).map_err(|_| StoreError::NumericRange)
}
#[cfg(debug_assertions)]
fn consume_failpoint(
    failpoint: &Arc<std::sync::Mutex<Option<Failpoint>>>,
    expected: Failpoint,
) -> Result<(), StoreError> {
    let mut guard = failpoint.lock().map_err(|_| StoreError::ActorUnavailable)?;
    if *guard == Some(expected) {
        *guard = None;
        return Err(StoreError::InjectedFailure);
    }
    Ok(())
}
