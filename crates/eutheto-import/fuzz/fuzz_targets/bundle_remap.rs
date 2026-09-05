#![no_main]

#[path = "../../../../tests/support/portable_decode.rs"]
mod portable_decode;
#[path = "../../../../tests/support/portable_encode.rs"]
mod portable_encode;

use eutheto_export::{
    ApplicationMetadata, BACKUP_SELECTION_EXTENSION, BackupSections, BackupSelection,
    BackupSelectionScope, ExportError, FixedExclusion, FullBackupSnapshot, PORTABLE_LIMITS,
    PortableBackupAssetSelection, assemble_full_backup, backup_selection_extension_value,
    canonical_json,
};
use eutheto_import::{
    CollisionAction, CollisionPlan, ImportError, ImportOptions, InspectionPolicy,
    LocalLibrarySnapshot, MigrationRegistries, RestoreMode, StagedDisposition, StagedImport,
    SupplementalCollisionAction, build_preview, inspect_bundle, stage_import,
};
use eutheto_types::{REVISION_MAX_V1, Revision, ScenarioSnapshotV1, SupplementalIdentity};
use libfuzzer_sys::fuzz_target;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const MAX_INPUT_BYTES: usize = 8 * 1024;
const SCENARIO_A: &str = "0195a5e4-7c00-7000-8000-000000000201";
const SCENARIO_B: &str = "0195a5e4-7c00-7000-8000-000000000202";
const ENTITY_A: &str = "0195a5e4-7c00-7000-8000-000000000211";
const ENTITY_B: &str = "0195a5e4-7c00-7000-8000-000000000212";
const SHARED_RECORD: &str = "0195a5e4-7c00-7000-8000-000000000221";
const RESULT_CURRENT: &str = "0195a5e4-7c00-7000-8000-000000000231";
const RESULT_HISTORICAL: &str = "0195a5e4-7c00-7000-8000-000000000232";

struct ReferenceCase {
    field: &'static str,
    value: Value,
    valid_shape: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RootState {
    Live,
    TombstonedOnly,
    Unseen,
    NestedReuse,
    Overflow,
}

fn root_state(data: &[u8]) -> RootState {
    match data.get(2).copied().unwrap_or(2) % 5 {
        0 => RootState::Live,
        1 => RootState::TombstonedOnly,
        2 => RootState::Unseen,
        3 => RootState::NestedReuse,
        _ => RootState::Overflow,
    }
}
fn reference_case(data: &[u8]) -> ReferenceCase {
    let fields = [
        ("participantId", false),
        ("participant_id", false),
        ("participant-id", false),
        ("locationId", false),
        ("location_id", false),
        ("location-id", false),
        ("activityId", false),
        ("activity_id", false),
        ("activity-id", false),
        ("vehicleIds", true),
        ("vehicle_ids", true),
        ("vehicle-ids", true),
    ];
    let (field, list) = fields[usize::from(data.first().copied().unwrap_or(0)) % fields.len()];
    let shape = data.get(1).copied().unwrap_or(0) % 4;
    let (value, valid_shape) = match (list, shape) {
        (false, 0) => (Value::String(ENTITY_B.to_owned()), true),
        (false, 1) => (Value::String(SCENARIO_B.to_owned()), true),
        (true, 0) => (json!([ENTITY_A, ENTITY_B]), true),
        (true, 1) => (json!([SCENARIO_A, SCENARIO_B]), true),
        (false, 2) => (json!([ENTITY_B]), false),
        (false, _) => (Value::from(7), false),
        (true, 2) => (Value::String(ENTITY_B.to_owned()), false),
        (true, _) => (json!([ENTITY_A, 7]), false),
    };
    ReferenceCase {
        field,
        value,
        valid_shape,
    }
}

fn marker(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(data.len() * 2);
    for byte in data {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn scenario(
    scenario_id: &str,
    entity_id: &str,
    linked_scenario_id: &str,
    linked_entity_id: &str,
    fuzz_marker: &str,
    reference: &ReferenceCase,
) -> Result<ScenarioSnapshotV1, serde_json::Error> {
    let mut document = json!({
        "format": "eutheto/scenario",
        "formatVersion": 1,
        "scenarioId": scenario_id,
        "domainPack": {"id": "official.test", "schemaVersion": 1},
        "metadata": {
            "title": "Bundle remap fuzz",
            "description": "",
            "createdAt": "2026-08-29T00:00:00Z",
            "updatedAt": "2026-08-29T00:00:00Z"
        },
        "settings": {
            "timeZone": "Etc/UTC",
            "locale": "en-US",
            "units": "metric",
            "horizon": {
                "start": "2026-08-29T00:00:00Z",
                "end": "2026-08-30T00:00:00Z"
            },
            "gapPolicy": "reject",
            "overlapPolicy": "earlier"
        },
        "domain": {
            "entities": {},
            "rules": {},
            "preferences": {},
            "lockedAssignments": {}
        },
        "extensions": {}
    });
    let mut record = json!({
        "id": entity_id,
        "scenarioId": linked_scenario_id,
        "entityId": linked_entity_id,
        "externalId": linked_entity_id,
        "note": format!("prose reference {linked_scenario_id} remains unchanged"),
        "fuzzMarker": fuzz_marker
    });
    record
        .as_object_mut()
        .expect("entity record object")
        .insert(reference.field.to_owned(), reference.value.clone());
    document["domain"]["entities"]
        .as_object_mut()
        .expect("entities object")
        .insert(entity_id.to_owned(), record);
    serde_json::from_value(json!({
        "format": "eutheto/scenario",
        "schemaVersion": 1,
        "revision": 7,
        "document": document,
        "requiredCapabilities": [],
        "semanticExtensions": {},
        "extensions": {}
    }))
}

fn remap_bundle(data: &[u8], reference: &ReferenceCase) -> Result<Vec<u8>, ExportError> {
    let marker = marker(data);
    let scenarios = vec![
        scenario(
            SCENARIO_A, ENTITY_A, SCENARIO_B, ENTITY_B, &marker, reference,
        )?,
        scenario(
            SCENARIO_B, ENTITY_B, SCENARIO_A, ENTITY_A, &marker, reference,
        )?,
    ];
    let mut historical_scenario = scenarios[0].clone();
    historical_scenario.revision = serde_json::from_value(json!(6))?;
    let mut sections = BackupSections::default();
    sections.results.insert(
        RESULT_CURRENT.to_owned(),
        json!({
            "resultId": RESULT_CURRENT,
            "scenarioId": SCENARIO_A,
            "scenarioRevision": 7,
            "payload": {"fuzzMarker": marker}
        }),
    );
    sections.results.insert(
        RESULT_HISTORICAL.to_owned(),
        json!({
            "resultId": RESULT_HISTORICAL,
            "scenarioId": SCENARIO_A,
            "scenarioRevision": 6,
            "payload": {"kind": "historical"}
        }),
    );
    sections.shared_records.insert(
        SHARED_RECORD.to_owned(),
        json!({
            "sharedRecordId": SHARED_RECORD,
            "scenarioIds": [SCENARIO_A, SCENARIO_B],
            "scenarioId": SCENARIO_A,
            "entityId": ENTITY_B
        }),
    );
    let backup_selection = BackupSelection {
        include_results: true,
        asset_selection: PortableBackupAssetSelection::All,
        threshold_version: None,
        threshold_bytes: None,
        excluded_asset_count: 0,
        excluded_asset_ids: BTreeSet::new(),
        fixed_exclusions: FixedExclusion::ALL.into_iter().collect(),
        scope: BackupSelectionScope::Library,
    };
    assemble_full_backup(
        &FullBackupSnapshot {
            bundle_id: serde_json::from_value(json!("0195a5e4-7c00-7000-8000-000000000222"))?,
            created_at: "2026-08-29T00:00:00Z".to_owned(),
            application: ApplicationMetadata {
                name: "eutheto-import-fuzz".to_owned(),
                version: "0".to_owned(),
            },
            title: "Bundle-wide remap".to_owned(),
            scenarios,
            scenario_revisions: vec![historical_scenario],
            sections,
            nonsemantic_extensions: BTreeSet::new(),
            manifest_extensions: BTreeMap::from([(
                BACKUP_SELECTION_EXTENSION.to_owned(),
                backup_selection_extension_value(&backup_selection)?,
            )]),
        },
        &portable_encode::encode_fixture_domain,
    )
}

fn bounded_policy() -> InspectionPolicy {
    let mut policy = InspectionPolicy::default();
    policy.limits = PORTABLE_LIMITS;
    policy.limits.max_archive_bytes = 256 * 1024;
    policy.limits.max_total_uncompressed_bytes = 256 * 1024;
    policy.limits.max_entry_bytes = 96 * 1024;
    policy.limits.max_entries = 12;
    policy.limits.max_compression_ratio = 8;
    policy.limits.max_path_bytes = 160;
    policy.limits.max_json_bytes = 96 * 1024;
    policy.limits.max_json_depth = 64;
    policy.limits.max_string_bytes = 32 * 1024;
    policy.limits.max_collection_items = 4_096;
    policy
}

fn declared_reference_kind(key: &str) -> Option<bool> {
    if key == "id" {
        return Some(false);
    }
    let normalized = key
        .chars()
        .filter(|character| !matches!(character, '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if normalized.ends_with("externalid") || normalized.ends_with("externalids") {
        return None;
    }
    if key.ends_with("Ids") || key.ends_with("_ids") || key.ends_with("-ids") {
        return Some(true);
    }
    if key.ends_with("Id") || key.ends_with("_id") || key.ends_with("-id") {
        return Some(false);
    }
    None
}

fn collect_declared_ids(value: &Value, ids: &mut BTreeSet<String>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_declared_ids(value, ids);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                match declared_reference_kind(key) {
                    Some(false) => {
                        if let Some(id) = value.as_str() {
                            ids.insert(id.to_owned());
                        }
                    }

                    Some(true) => {
                        if let Some(values) = value.as_array() {
                            ids.extend(values.iter().filter_map(Value::as_str).map(str::to_owned));
                        }
                    }
                    None => collect_declared_ids(value, ids),
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}
fn collect_stale_paths(
    value: &Value,
    path: &str,
    old_ids: &BTreeSet<String>,
    stale: &mut Vec<String>,
) {
    match value {
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_stale_paths(value, &format!("{path}/{index}"), old_ids, stale);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                match declared_reference_kind(key) {
                    Some(false) => {
                        if value
                            .as_str()
                            .is_some_and(|identity| old_ids.contains(identity))
                        {
                            stale.push(format!("{path}/{key}"));
                        }
                    }
                    Some(true) => {
                        if let Some(items) = value.as_array() {
                            for (index, item) in items.iter().enumerate() {
                                if item
                                    .as_str()
                                    .is_some_and(|identity| old_ids.contains(identity))
                                {
                                    stale.push(format!("{path}/{key}/{index}"));
                                }
                            }
                        }
                    }
                    None => collect_stale_paths(value, &format!("{path}/{key}"), old_ids, stale),
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn stale_declared_paths(staged: &StagedImport, old_ids: &BTreeSet<String>) -> Vec<String> {
    let mut stale = Vec::new();
    for scenario in &staged.scenarios {
        let domain =
            serde_json::to_value(&scenario.scenario.document.domain).expect("serialize domain");
        collect_stale_paths(
            &domain,
            &format!("scenarios/{}/domain", scenario.original_id),
            old_ids,
            &mut stale,
        );
    }
    for scenario in &staged.scenario_revisions {
        let domain = serde_json::to_value(&scenario.document.domain).expect("serialize revision");
        collect_stale_paths(
            &domain,
            &format!(
                "scenario-revisions/{}-{}",
                scenario.document.scenario_id,
                scenario.revision.value()
            ),
            old_ids,
            &mut stale,
        );
    }
    for (section_name, section) in [
        ("results", &staged.results),
        ("shared", &staged.shared_records),
        ("preferences", &staged.preferences),
    ] {
        for (key, bytes) in section {
            let value: Value = serde_json::from_slice(bytes).expect("staged JSON");
            collect_stale_paths(
                &value,
                &format!("{section_name}/{key}"),
                old_ids,
                &mut stale,
            );
        }
    }
    stale
}

fn staged_declared_ids(staged: &StagedImport) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for scenario in &staged.scenarios {
        let domain = serde_json::to_value(&scenario.scenario.document.domain)
            .expect("serialize staged domain");
        collect_declared_ids(&domain, &mut ids);
        let extensions = serde_json::to_value(&scenario.scenario.semantic_extensions)
            .expect("serialize staged semantic extensions");
        collect_declared_ids(&extensions, &mut ids);
    }
    for scenario in &staged.scenario_revisions {
        let domain =
            serde_json::to_value(&scenario.document.domain).expect("serialize staged revision");
        collect_declared_ids(&domain, &mut ids);
        let extensions = serde_json::to_value(&scenario.semantic_extensions)
            .expect("serialize staged revision extensions");
        collect_declared_ids(&extensions, &mut ids);
    }
    for section in [&staged.results, &staged.shared_records, &staged.preferences] {
        for bytes in section.values() {
            let value: Value = serde_json::from_slice(bytes).expect("staged JSON remains valid");
            collect_declared_ids(&value, &mut ids);
        }
    }
    ids
}

fn collect_preserved_positions(
    scenario: &ScenarioSnapshotV1,
    external_ids: &mut BTreeSet<String>,
    prose: &mut BTreeSet<String>,
) {
    let domain =
        serde_json::to_value(&scenario.document.domain).expect("serialize preservation oracle");
    let entities = domain
        .get("entities")
        .and_then(Value::as_object)
        .expect("entities remain an object");
    for record in entities.values().filter_map(Value::as_object) {
        if let Some(external_id) = record.get("externalId").and_then(Value::as_str) {
            external_ids.insert(external_id.to_owned());
        }
        if let Some(note) = record.get("note").and_then(Value::as_str) {
            prose.insert(note.to_owned());
        }
    }
}

fn assert_external_and_prose_preserved(staged: &StagedImport) {
    let mut external_ids = BTreeSet::new();
    let mut prose = BTreeSet::new();
    for scenario in &staged.scenarios {
        collect_preserved_positions(&scenario.scenario, &mut external_ids, &mut prose);
    }
    for scenario in &staged.scenario_revisions {
        collect_preserved_positions(scenario, &mut external_ids, &mut prose);
    }
    assert_eq!(
        external_ids,
        BTreeSet::from([ENTITY_A.to_owned(), ENTITY_B.to_owned()])
    );
    assert_eq!(
        prose,
        BTreeSet::from([
            format!("prose reference {SCENARIO_A} remains unchanged"),
            format!("prose reference {SCENARIO_B} remains unchanged"),
        ])
    );
}

fn assert_deterministic(left: &StagedImport, right: &StagedImport) {
    assert_eq!(left.binding, right.binding);
    assert_eq!(left.mode, right.mode);
    assert_eq!(left.results, right.results);
    assert_eq!(left.shared_records, right.shared_records);
    assert_eq!(left.preferences, right.preferences);
    assert_eq!(left.assets, right.assets);
    assert_eq!(
        left.supplemental_replacements,
        right.supplemental_replacements
    );
    assert_eq!(
        canonical_json(&left.scenario_revisions).expect("serialize staged revisions"),
        canonical_json(&right.scenario_revisions).expect("serialize repeated staged revisions")
    );
    assert_eq!(
        left.provenance.source_bundle_id,
        right.provenance.source_bundle_id
    );
    assert_eq!(
        left.provenance.source_application,
        right.provenance.source_application
    );
    assert_eq!(
        left.provenance.original_format_version,
        right.provenance.original_format_version
    );
    assert_eq!(
        left.provenance.original_schema_version,
        right.provenance.original_schema_version
    );
    assert_eq!(
        left.provenance.source_file_sha256,
        right.provenance.source_file_sha256
    );
    assert_eq!(
        left.provenance.applied_migrations,
        right.provenance.applied_migrations
    );
    assert_eq!(left.scenarios.len(), right.scenarios.len());
    for (left, right) in left.scenarios.iter().zip(&right.scenarios) {
        assert_eq!(left.original_id, right.original_id);
        assert_eq!(left.disposition, right.disposition);
        assert_eq!(left.id_remap, right.id_remap);
        assert_eq!(
            canonical_json(&left.scenario).expect("serialize first staged scenario"),
            canonical_json(&right.scenario).expect("serialize repeated staged scenario")
        );
    }
}

fn supplemental_identity(section: &str, id: &str) -> SupplementalIdentity {
    serde_json::from_value(json!({
        "section": section,
        "key": format!("{id}.json")
    }))
    .expect("supplemental identity fixture")
}

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }
    let reference = reference_case(data);
    let root_state = root_state(data);
    let bytes = match remap_bundle(data, &reference) {
        Ok(bytes) => bytes,
        Err(_typed_refusal) if !reference.valid_shape => return,
        Err(error) => panic!("valid declared reference fixture was rejected: {error}"),
    };
    let policy = bounded_policy();
    let inspected = match inspect_bundle(
        &bytes,
        &policy,
        &MigrationRegistries::current_only(),
        &portable_decode::decode_fixture_domain,
    ) {
        Ok(inspected) => inspected,
        Err(_typed_refusal) if !reference.valid_shape => return,
        Err(error) => panic!("valid declared reference bundle was rejected: {error}"),
    };
    let scenario_a = inspected
        .scenarios
        .iter()
        .find(|scenario| scenario.document.scenario_id.to_string() == SCENARIO_A)
        .expect("scenario A inspected")
        .document
        .scenario_id;
    let scenario_b = inspected
        .scenarios
        .iter()
        .find(|scenario| scenario.document.scenario_id.to_string() == SCENARIO_B)
        .expect("scenario B inspected")
        .document
        .scenario_id;
    let shared_identity = supplemental_identity("shared-records", SHARED_RECORD);
    let current_result_identity = supplemental_identity("results", RESULT_CURRENT);
    let historical_result_identity = supplemental_identity("results", RESULT_HISTORICAL);
    let supplemental_identities = BTreeSet::from([
        shared_identity.clone(),
        current_result_identity.clone(),
        historical_result_identity.clone(),
    ]);
    let revision_floor = Revision::new(if data.get(3).copied().unwrap_or(1) & 1 == 0 {
        3
    } else {
        9
    });
    let mut scenario_ids = BTreeSet::from([scenario_b]);
    let mut scenario_revision_high_water = BTreeMap::new();
    let mut occupied_uuids = BTreeSet::from([SCENARIO_B.to_owned(), ENTITY_B.to_owned()]);
    match root_state {
        RootState::Live => {
            scenario_ids.insert(scenario_a);
            scenario_revision_high_water.insert(scenario_a, revision_floor);
            occupied_uuids.insert(SCENARIO_A.to_owned());
            occupied_uuids.insert(ENTITY_A.to_owned());
        }
        RootState::TombstonedOnly => {
            scenario_revision_high_water.insert(scenario_a, revision_floor);
            occupied_uuids.insert(SCENARIO_A.to_owned());
        }
        RootState::Unseen => {}
        RootState::NestedReuse => {
            occupied_uuids.insert(ENTITY_A.to_owned());
        }
        RootState::Overflow => {
            scenario_revision_high_water.insert(
                scenario_a,
                Revision::try_new(REVISION_MAX_V1).expect("maximum v1 revision"),
            );
            occupied_uuids.insert(SCENARIO_A.to_owned());
        }
    }
    let expected_scenario_ids = scenario_ids.clone();
    let expected_high_water = scenario_revision_high_water.clone();
    let expected_occupied = occupied_uuids.clone();
    let local = LocalLibrarySnapshot {
        revision: Revision::new(11),
        scenario_ids,
        scenario_revision_high_water,
        identity_owners: BTreeMap::new(),
        occupied_uuids,
        scenarios: Vec::new(),
        supplemental_identities: supplemental_identities.clone(),
        supplemental_identity_owners: BTreeMap::new(),
        settings: BTreeMap::new(),
    };
    let options = ImportOptions {
        restore_mode: RestoreMode::AddBackup,
        include_results: true,
        include_assets: true,
    };
    let preview = match build_preview(&inspected, &options, &local) {
        Err(ImportError::RevisionOverflow { scenario_id }) if root_state == RootState::Overflow => {
            assert_eq!(scenario_id, scenario_a);
            return;
        }
        Err(error) => panic!("revision-aware preview was rejected: {error}"),
        Ok(_) if root_state == RootState::Overflow => {
            panic!("maximum tombstone revision did not refuse import")
        }
        Ok(preview) => preview,
    };
    let preview_again =
        build_preview(&inspected, &options, &local).expect("build deterministic remap preview");
    assert_eq!(preview.binding, preview_again.binding);
    assert_eq!(preview.scenarios.len(), 2);
    let scenario_a_preview = preview
        .scenarios
        .iter()
        .find(|scenario| scenario.scenario_id == scenario_a)
        .expect("scenario A preview");
    assert_eq!(scenario_a_preview.source_revision.value(), 7);
    let expected_a_collision = matches!(root_state, RootState::Live | RootState::NestedReuse);
    assert_eq!(scenario_a_preview.collides, expected_a_collision);
    match root_state {
        RootState::TombstonedOnly => {
            let expected = if revision_floor.value() < 7 {
                7
            } else {
                revision_floor.value() + 1
            };
            assert_eq!(scenario_a_preview.same_identity_revision.value(), expected);
            assert_eq!(
                scenario_a_preview.same_identity_revision_warning.is_some(),
                revision_floor.value() >= 7
            );
        }
        RootState::Unseen | RootState::NestedReuse => {
            assert_eq!(scenario_a_preview.same_identity_revision.value(), 7);
        }
        RootState::Live => {
            assert_eq!(
                scenario_a_preview.same_identity_revision.value(),
                revision_floor.value().max(7) + u64::from(revision_floor.value() >= 7)
            );
        }
        RootState::Overflow => unreachable!(),
    }
    assert!(
        preview
            .scenarios
            .iter()
            .find(|scenario| scenario.scenario_id == scenario_b)
            .expect("scenario B preview")
            .collides
    );
    assert_eq!(
        preview
            .supplemental_collisions
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        supplemental_identities
    );

    let before_scenarios = canonical_json(&inspected.scenarios).expect("snapshot scenarios");
    let before_revisions =
        canonical_json(&inspected.scenario_revisions).expect("snapshot scenario revisions");
    let before_entries = inspected.additional_entries.clone();
    let failure = stage_import(
        &inspected,
        &preview,
        &options,
        &local,
        &CollisionPlan::default(),
    );
    assert!(failure.is_err(), "incomplete collision plan was accepted");
    assert_eq!(
        before_scenarios,
        canonical_json(&inspected.scenarios).expect("scenarios remain unchanged")
    );
    assert_eq!(
        before_revisions,
        canonical_json(&inspected.scenario_revisions).expect("scenario revisions remain unchanged")
    );
    assert_eq!(before_entries, inspected.additional_entries);
    assert_eq!(local.scenario_ids, expected_scenario_ids);
    assert_eq!(local.scenario_revision_high_water, expected_high_water);
    assert_eq!(local.revision.value(), 11);
    assert_eq!(local.occupied_uuids, expected_occupied);
    assert!(local.settings.is_empty());
    assert_eq!(local.supplemental_identities, supplemental_identities);

    let mut scenario_plan = BTreeMap::from([(scenario_b, CollisionAction::CreateCopy)]);
    if expected_a_collision {
        scenario_plan.insert(scenario_a, CollisionAction::CreateCopy);
    }
    let plan = CollisionPlan {
        scenarios: scenario_plan,
        supplemental: BTreeMap::from([
            (shared_identity, SupplementalCollisionAction::Replace),
            (
                current_result_identity,
                SupplementalCollisionAction::Replace,
            ),
            (
                historical_result_identity,
                SupplementalCollisionAction::Replace,
            ),
        ]),
    };
    let staged = match stage_import(&inspected, &preview, &options, &local, &plan) {
        Ok(_) if !reference.valid_shape => {
            panic!("malformed declared reference shape was accepted")
        }
        Ok(staged) => staged,
        Err(_typed_refusal) if !reference.valid_shape => {
            assert_eq!(
                before_scenarios,
                canonical_json(&inspected.scenarios).expect("failed stage remains non-mutating")
            );
            assert_eq!(
                before_revisions,
                canonical_json(&inspected.scenario_revisions)
                    .expect("failed revision stage remains non-mutating")
            );
            assert_eq!(local.occupied_uuids, expected_occupied);
            assert_eq!(local.scenario_revision_high_water, expected_high_water);
            assert_eq!(before_entries, inspected.additional_entries);
            return;
        }
        Err(error) => panic!("valid declared reference staging was rejected: {error}"),
    };
    let staged_again = stage_import(&inspected, &preview_again, &options, &local, &plan)
        .expect("repeat valid bundle-wide remap");
    let staged_a = staged
        .scenarios
        .iter()
        .find(|scenario| scenario.original_id == scenario_a)
        .expect("scenario A staged");
    assert_eq!(
        staged_a.disposition,
        if expected_a_collision {
            StagedDisposition::CreateCopy
        } else {
            StagedDisposition::Create
        }
    );
    if root_state == RootState::NestedReuse {
        let remapped = staged_a
            .id_remap
            .keys()
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        assert!(remapped.contains(SCENARIO_A));
        assert!(remapped.contains(ENTITY_A));
    }
    assert_eq!(
        staged_a.scenario.revision.value(),
        if expected_a_collision {
            scenario_a_preview.source_revision.value()
        } else {
            scenario_a_preview.same_identity_revision.value()
        }
    );
    assert!(
        staged
            .scenario_revisions
            .iter()
            .any(|revision| revision.revision.value() == 6
                && revision.document.scenario_id == staged_a.scenario.document.scenario_id)
    );
    assert_deterministic(&staged, &staged_again);

    let (staged_shared_key, staged_shared_bytes) = staged
        .shared_records
        .first_key_value()
        .expect("included shared record staged");
    assert_eq!(staged.shared_records.len(), 1);
    assert_eq!(staged_shared_key, &format!("{SHARED_RECORD}.json"));
    assert_eq!(staged.results.len(), 2);
    let mut staged_result_ids = BTreeSet::new();
    for (key, bytes) in &staged.results {
        let result_id = key
            .strip_suffix(".json")
            .expect("staged result key keeps canonical JSON suffix");
        let result: Value = serde_json::from_slice(bytes).expect("staged result remains JSON");
        assert_eq!(
            result.get("resultId").and_then(Value::as_str),
            Some(result_id)
        );
        staged_result_ids.insert(result_id.to_owned());
    }
    let source_result_ids =
        BTreeSet::from([RESULT_CURRENT.to_owned(), RESULT_HISTORICAL.to_owned()]);
    if expected_a_collision {
        assert!(staged_result_ids.is_disjoint(&source_result_ids));
    } else {
        assert_eq!(staged_result_ids, source_result_ids);
    }

    let mut old_ids = staged
        .scenarios
        .iter()
        .flat_map(|scenario| scenario.id_remap.keys().map(ToString::to_string))
        .collect::<BTreeSet<_>>();
    let mut mapped_new_ids = staged
        .scenarios
        .iter()
        .flat_map(|scenario| scenario.id_remap.values().map(ToString::to_string))
        .collect::<BTreeSet<_>>();
    if expected_a_collision {
        old_ids.extend(source_result_ids);
        mapped_new_ids.extend(staged_result_ids.iter().cloned());
    }
    assert_eq!(old_ids.len(), mapped_new_ids.len());

    let mut identity_set = mapped_new_ids;
    identity_set.extend(staged_result_ids);
    identity_set.insert(SHARED_RECORD.to_owned());
    for scenario in &staged.scenarios {
        identity_set.insert(scenario.scenario.document.scenario_id.to_string());
        identity_set.extend(
            scenario
                .scenario
                .document
                .domain
                .entities
                .keys()
                .map(ToString::to_string),
        );
        identity_set.extend(
            scenario
                .scenario
                .document
                .domain
                .rules
                .keys()
                .map(ToString::to_string),
        );
        identity_set.extend(
            scenario
                .scenario
                .document
                .domain
                .preferences
                .keys()
                .map(ToString::to_string),
        );
        identity_set.extend(
            scenario
                .scenario
                .document
                .domain
                .locked_assignments
                .keys()
                .map(ToString::to_string),
        );
    }
    assert!(old_ids.is_disjoint(&identity_set));
    let declared_ids = staged_declared_ids(&staged);
    let stale_declared = declared_ids
        .intersection(&old_ids)
        .cloned()
        .collect::<BTreeSet<_>>();
    let stale_paths = stale_declared_paths(&staged, &old_ids);
    assert!(
        stale_declared.is_empty(),
        "mapped old IDs survived in declared references: {stale_declared:?} at {stale_paths:?}"
    );
    assert!(declared_ids.is_subset(&identity_set));
    assert_external_and_prose_preserved(&staged);

    let represented = staged
        .scenarios
        .iter()
        .map(|scenario| scenario.scenario.document.scenario_id.to_string())
        .collect::<BTreeSet<_>>();
    assert!(represented.is_subset(&identity_set));
    let shared: Value =
        serde_json::from_slice(staged_shared_bytes).expect("shared record remains JSON");
    let mut shared_ids = BTreeSet::new();
    collect_declared_ids(&shared, &mut shared_ids);
    assert!(represented.is_subset(&shared_ids));
});
