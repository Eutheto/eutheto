#![no_main]

#[path = "../../../../tests/support/portable_decode.rs"]
mod portable_decode;

use eutheto_export::{
    CHECKSUM_ALGORITHM, CHECKSUMS_PATH, Checksums, MANIFEST_PATH, PORTABLE_LIMITS, canonical_json,
    sha256_hex,
};
use eutheto_import::{InspectionPolicy, MigrationRegistries, inspect_bundle};
use libfuzzer_sys::fuzz_target;
use serde_json::json;
use std::collections::BTreeMap;
use std::error::Error;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const MAX_SCENARIO_BYTES: usize = 64 * 1024;
const SCENARIO_PATH: &str = "scenarios/0195a5e4-7c00-7000-8000-000000000001.json";

fn bounded_policy() -> InspectionPolicy {
    let mut policy = InspectionPolicy::default();
    policy.limits = PORTABLE_LIMITS;
    policy.limits.max_archive_bytes = 128 * 1024;
    policy.limits.max_total_uncompressed_bytes = 128 * 1024;
    policy.limits.max_entry_bytes = MAX_SCENARIO_BYTES as u64;
    policy.limits.max_entries = 3;
    policy.limits.max_compression_ratio = 2;
    policy.limits.max_path_bytes = 96;
    policy.limits.max_json_bytes = MAX_SCENARIO_BYTES as u64;
    policy.limits.max_json_depth = 64;
    policy.limits.max_string_bytes = 32 * 1024;
    policy.limits.max_collection_items = 4_096;
    policy
}

fn wrap_scenario(data: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let manifest_bytes = canonical_json(&json!({
        "format": "eutheto-bundle",
        "formatVersion": 1,
        "schemaVersion": 2,
        "bundleId": "0195a5e4-7c00-7000-8000-000000000010",
        "bundleKind": "scenario-export",
        "createdAt": "2026-08-29T12:00:00Z",
        "application": { "name": "eutheto-import-fuzz", "version": "0" },
        "title": "fuzz scenario",
        "counts": {
            "scenarios": 1,
            "results": 0,
            "sharedRecords": 0,
            "preferences": 0,
            "assets": 0
        },
        "requiredCapabilities": [{"id": "official.test.portable", "version": 2}],
        "nonsemanticExtensions": [],
        "integrity": {
            "algorithm": CHECKSUM_ALGORITHM,
            "checksumsFile": CHECKSUMS_PATH
        },
        "extensions": {}
    }))?;
    let checksums = Checksums {
        algorithm: CHECKSUM_ALGORITHM.to_owned(),
        files: BTreeMap::from([
            (MANIFEST_PATH.to_owned(), sha256_hex(&manifest_bytes)),
            (SCENARIO_PATH.to_owned(), sha256_hex(data)),
        ]),
    };
    let checksums_bytes = canonical_json(&checksums)?;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o600);
    for (path, bytes) in [
        (CHECKSUMS_PATH, checksums_bytes.as_slice()),
        (MANIFEST_PATH, manifest_bytes.as_slice()),
        (SCENARIO_PATH, data),
    ] {
        writer.start_file(path, options)?;
        writer.write_all(bytes)?;
    }
    Ok(writer.finish()?.into_inner())
}

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_SCENARIO_BYTES {
        return;
    }

    if let Ok(bundle) = wrap_scenario(data) {
        let policy = bounded_policy();
        let registries = MigrationRegistries::current_only();
        let _inspection = inspect_bundle(
            &bundle,
            &policy,
            &registries,
            &portable_decode::decode_fixture_domain,
        );
    }
});
