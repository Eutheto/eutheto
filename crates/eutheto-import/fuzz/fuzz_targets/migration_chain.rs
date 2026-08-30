#![no_main]

use eutheto_export::{
    ApplicationMetadata, BackupSections, CHECKSUM_ALGORITHM, CHECKSUMS_PATH,
    CURRENT_BUNDLE_FORMAT_VERSION, CURRENT_PORTABLE_SCHEMA_VERSION, Checksums, MANIFEST_PATH,
    PORTABLE_LIMITS, PortableScenario, ScenarioExportSnapshot, assemble_scenario_export,
    canonical_json, sha256_hex,
};
use eutheto_import::{
    ImportError, InspectionPolicy, LogicalBundle, MigrationFailure, MigrationRegistries,
    OuterMigrationStep, PortableMigrationStep, inspect_bundle,
};
use libfuzzer_sys::fuzz_target;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const MAX_INPUT_BYTES: usize = 8 * 1024;
const SCENARIO_ID: &str = "0195a5e4-7c00-7000-8000-000000000101";

fn marker(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(data.len() * 2);
    for byte in data {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn current_bundle(data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let scenario: PortableScenario = serde_json::from_value(json!({
        "format": "eutheto/scenario",
        "schemaVersion": CURRENT_PORTABLE_SCHEMA_VERSION,
        "revision": 1,
        "document": {
            "format": "eutheto/scenario",
            "formatVersion": 1,
            "scenarioId": SCENARIO_ID,
            "domainPack": {"id": "official.generic", "schemaVersion": 1},
            "metadata": {
                "title": "Migration fuzz",
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
            "extensions": {"fuzz.marker": {"bytes": marker(data)}}
        },
        "requiredCapabilities": [],
        "semanticExtensions": {},
        "extensions": {}
    }))?;
    Ok(assemble_scenario_export(&ScenarioExportSnapshot {
        bundle_id: "0195a5e4-7c00-7000-8000-000000000111".parse()?,
        created_at: "2026-08-29T00:00:00Z".to_owned(),
        application: ApplicationMetadata {
            name: "eutheto-import-fuzz".to_owned(),
            version: "0".to_owned(),
        },
        title: "Migration chain".to_owned(),
        scenario,
        scenario_revisions: Vec::new(),
        sections: BackupSections::default(),
        nonsemantic_extensions: BTreeSet::new(),
        manifest_extensions: BTreeMap::new(),
    })?)
}

fn rebuild_old_bundle(
    current: &[u8],
    policy: &InspectionPolicy,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let inspected = inspect_bundle(current, policy, &MigrationRegistries::current_only())?;
    let mut manifest = inspected.manifest;
    manifest.format_version = 0;
    manifest.schema_version = 0;
    let mut scenario = serde_json::to_value(&inspected.scenarios[0])?;
    scenario["schemaVersion"] = Value::from(0);
    let scenario_path = format!("scenarios/{SCENARIO_ID}.json");
    let manifest_bytes = canonical_json(&manifest)?;
    let scenario_bytes = canonical_json(&scenario)?;
    let checksum_bytes = canonical_json(&Checksums {
        algorithm: CHECKSUM_ALGORITHM.to_owned(),
        files: BTreeMap::from([
            (MANIFEST_PATH.to_owned(), sha256_hex(&manifest_bytes)),
            (scenario_path.clone(), sha256_hex(&scenario_bytes)),
        ]),
    })?;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o600);
    for (path, bytes) in [
        (CHECKSUMS_PATH, checksum_bytes.as_slice()),
        (MANIFEST_PATH, manifest_bytes.as_slice()),
        (scenario_path.as_str(), scenario_bytes.as_slice()),
    ] {
        writer.start_file(path, options)?;
        writer.write_all(bytes)?;
    }
    Ok(writer.finish()?.into_inner())
}

fn migrate_outer_v0(mut bundle: LogicalBundle) -> Result<LogicalBundle, MigrationFailure> {
    let manifest = bundle
        .manifest
        .as_object_mut()
        .ok_or_else(|| MigrationFailure {
            message: "manifest must be an object".to_owned(),
        })?;
    manifest.insert(
        "formatVersion".to_owned(),
        Value::from(CURRENT_BUNDLE_FORMAT_VERSION),
    );
    Ok(bundle)
}

fn migrate_portable_v0(mut value: Value) -> Result<Value, MigrationFailure> {
    let scenario = value.as_object_mut().ok_or_else(|| MigrationFailure {
        message: "scenario must be an object".to_owned(),
    })?;
    scenario.insert(
        "schemaVersion".to_owned(),
        Value::from(CURRENT_PORTABLE_SCHEMA_VERSION),
    );
    Ok(value)
}

fn fail_outer(_bundle: LogicalBundle) -> Result<LogicalBundle, MigrationFailure> {
    Err(MigrationFailure {
        message: "intentional typed migration failure".to_owned(),
    })
}

fn bounded_policy() -> InspectionPolicy {
    let mut policy = InspectionPolicy::default();
    policy.limits = PORTABLE_LIMITS;
    policy.limits.max_archive_bytes = 192 * 1024;
    policy.limits.max_total_uncompressed_bytes = 192 * 1024;
    policy.limits.max_entry_bytes = 96 * 1024;
    policy.limits.max_entries = 4;
    policy.limits.max_compression_ratio = 4;
    policy.limits.max_path_bytes = 128;
    policy.limits.max_json_bytes = 96 * 1024;
    policy.limits.max_json_depth = 64;
    policy.limits.max_string_bytes = 32 * 1024;
    policy.limits.max_collection_items = 4_096;
    policy
}

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }
    assert_eq!(
        CURRENT_BUNDLE_FORMAT_VERSION, 1,
        "advance the explicit historical migration chain with the format version"
    );
    assert_eq!(
        CURRENT_PORTABLE_SCHEMA_VERSION, 1,
        "advance the explicit historical migration chain with the schema version"
    );
    let policy = bounded_policy();
    let current_bytes = current_bundle(data).expect("construct current migration bundle");
    let old_bytes =
        rebuild_old_bundle(&current_bytes, &policy).expect("construct historical migration bundle");
    let registries = MigrationRegistries::new(
        vec![OuterMigrationStep {
            from_version: 0,
            to_version: CURRENT_BUNDLE_FORMAT_VERSION,
            name: "outer-v0-to-v1",
            migrate: migrate_outer_v0,
        }],
        vec![PortableMigrationStep {
            from_version: 0,
            to_version: CURRENT_PORTABLE_SCHEMA_VERSION,
            name: "portable-v0-to-v1",
            migrate: migrate_portable_v0,
        }],
    )
    .expect("register complete sequential historical migration chain");

    let migrated =
        inspect_bundle(&old_bytes, &policy, &registries).expect("valid historical chain");
    let migrated_again =
        inspect_bundle(&old_bytes, &policy, &registries).expect("deterministic historical chain");
    let current = inspect_bundle(
        &current_bytes,
        &policy,
        &MigrationRegistries::current_only(),
    )
    .expect("valid current bundle");

    assert_eq!(migrated.original_format_version, 0);
    assert_eq!(migrated.original_schema_version, 0);
    assert_eq!(migrated.applied_migrations.len(), 2);
    assert_eq!(
        canonical_json(&migrated.scenarios).expect("serialize migrated scenarios"),
        canonical_json(&current.scenarios).expect("serialize current scenarios")
    );
    assert_eq!(
        canonical_json(&migrated.scenarios).expect("serialize migrated scenarios"),
        canonical_json(&migrated_again.scenarios).expect("serialize repeated migration")
    );
    assert_eq!(
        migrated.applied_migrations,
        migrated_again.applied_migrations
    );
    assert_eq!(
        canonical_json(&migrated.manifest).expect("serialize migrated manifest"),
        canonical_json(&migrated_again.manifest).expect("serialize repeated manifest")
    );
    assert_eq!(
        canonical_json(&migrated.scenario_revisions).expect("serialize migrated revisions"),
        canonical_json(&migrated_again.scenario_revisions)
            .expect("serialize repeated migrated revisions")
    );
    assert_eq!(migrated.checksums, migrated_again.checksums);
    assert_eq!(
        migrated.additional_entries,
        migrated_again.additional_entries
    );
    assert_eq!(migrated.file_sha256, migrated_again.file_sha256);

    if data.first().is_some_and(|byte| byte & 1 != 0) {
        let failing = MigrationRegistries::new(
            vec![OuterMigrationStep {
                from_version: 0,
                to_version: CURRENT_BUNDLE_FORMAT_VERSION,
                name: "typed-failure",
                migrate: fail_outer,
            }],
            vec![PortableMigrationStep {
                from_version: 0,
                to_version: CURRENT_PORTABLE_SCHEMA_VERSION,
                name: "portable-v0-to-v1-before-failure",
                migrate: migrate_portable_v0,
            }],
        )
        .expect("complete registry with intentional outer failure");
        match inspect_bundle(&old_bytes, &policy, &failing) {
            Ok(_) => panic!("an intentionally failing migration must not be accepted"),
            Err(ImportError::Migration(message)) => {
                assert!(message.contains("intentional typed migration failure"));
            }
            Err(_typed_refusal) => {}
        }
    }
});
