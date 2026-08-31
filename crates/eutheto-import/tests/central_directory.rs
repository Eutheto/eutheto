use eutheto_export::{
    ApplicationMetadata, BackupSections, PortableScenario, ScenarioExportSnapshot,
    assemble_scenario_export,
};
use eutheto_import::{
    ImportError, InspectedBundle, InspectionPolicy, MigrationRegistries, inspect_bundle,
};
use eutheto_types::{BundleId, Revision, ScenarioDocument};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use uuid::Uuid;

const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;
const CENTRAL_DIGITAL_SIGNATURE: u32 = 0x0505_4b50;
const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const ZIP64_EOCD_SIGNATURE: u32 = 0x0606_4b50;
const ZIP64_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
const UTF8_NAME: u16 = 1 << 11;
const ENCRYPTED: u16 = 1;
const VERSION_NEEDED: u16 = 20;
const ZIP64_VERSION_NEEDED: u16 = 45;
const UNIX_VERSION_MADE_BY: u16 = (3 << 8) | VERSION_NEEDED;
const DOS_DATE_1980_01_01: u16 = (1 << 5) | 1;

struct BuiltZip {
    bytes: Vec<u8>,
    local_headers: Vec<usize>,
    central_headers: Vec<usize>,
    central_offset: usize,
    central_size: usize,
    eocd: usize,
}

fn inspect(bytes: &[u8]) -> Result<InspectedBundle, ImportError> {
    inspect_bundle(
        bytes,
        &InspectionPolicy::default(),
        &MigrationRegistries::current_only(),
    )
}

fn assert_invalid_structure(bytes: &[u8], case: &str) {
    let result = inspect(bytes);
    assert!(
        matches!(&result, Err(ImportError::InvalidZipStructure(_))),
        "{case}: {result:?}"
    );
}

fn assert_unsupported_feature(bytes: &[u8], case: &str) {
    let result = inspect(bytes);
    assert!(
        matches!(&result, Err(ImportError::UnsupportedZipFeature(_))),
        "{case}: {result:?}"
    );
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn set_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn stored_zip(entries: &[(&[u8], &[u8])]) -> Result<BuiltZip, Box<dyn Error>> {
    let mut bytes = Vec::new();
    let mut local_headers = Vec::with_capacity(entries.len());
    let mut central_records = Vec::with_capacity(entries.len());

    for &(name, content) in entries {
        let name_len = u16::try_from(name.len())?;
        let content_len = u32::try_from(content.len())?;
        let local_offset = u32::try_from(bytes.len())?;
        let checksum = crc32(content);
        local_headers.push(bytes.len());

        push_u32(&mut bytes, LOCAL_HEADER_SIGNATURE);
        push_u16(&mut bytes, VERSION_NEEDED);
        push_u16(&mut bytes, UTF8_NAME);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, DOS_DATE_1980_01_01);
        push_u32(&mut bytes, checksum);
        push_u32(&mut bytes, content_len);
        push_u32(&mut bytes, content_len);
        push_u16(&mut bytes, name_len);
        push_u16(&mut bytes, 0);
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(content);

        let mut central = Vec::new();
        push_u32(&mut central, CENTRAL_HEADER_SIGNATURE);
        push_u16(&mut central, UNIX_VERSION_MADE_BY);
        push_u16(&mut central, VERSION_NEEDED);
        push_u16(&mut central, UTF8_NAME);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u16(&mut central, DOS_DATE_1980_01_01);
        push_u32(&mut central, checksum);
        push_u32(&mut central, content_len);
        push_u32(&mut central, content_len);
        push_u16(&mut central, name_len);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u32(&mut central, 0o100_600_u32 << 16);
        push_u32(&mut central, local_offset);
        central.extend_from_slice(name);
        central_records.push(central);
    }

    let central_offset = bytes.len();
    let mut central_headers = Vec::with_capacity(central_records.len());
    for central in central_records {
        central_headers.push(bytes.len());
        bytes.extend_from_slice(&central);
    }
    let central_size = bytes.len() - central_offset;
    let eocd = bytes.len();
    let entry_count = u16::try_from(entries.len())?;

    push_u32(&mut bytes, EOCD_SIGNATURE);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, entry_count);
    push_u16(&mut bytes, entry_count);
    push_u32(&mut bytes, u32::try_from(central_size)?);
    push_u32(&mut bytes, u32::try_from(central_offset)?);
    push_u16(&mut bytes, 0);

    Ok(BuiltZip {
        bytes,
        local_headers,
        central_headers,
        central_offset,
        central_size,
        eocd,
    })
}

fn one_entry_zip() -> Result<BuiltZip, Box<dyn Error>> {
    stored_zip(&[(b"entry.txt", b"contents")])
}

#[test]
fn duplicate_raw_filenames_are_rejected_before_zip_reader_collapse() -> Result<(), Box<dyn Error>> {
    let archive = stored_zip(&[(b"same.txt", b"first"), (b"same.txt", b"second")])?;
    let result = inspect(&archive.bytes);
    assert!(
        matches!(&result, Err(ImportError::DuplicatePath(path)) if path == "same.txt"),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn forged_eocd_entry_counts_are_invalid() -> Result<(), Box<dyn Error>> {
    let archive = one_entry_zip()?;

    let mut excess = archive.bytes.clone();
    set_u16(&mut excess, archive.eocd + 8, 2);
    set_u16(&mut excess, archive.eocd + 10, 2);
    assert_invalid_structure(&excess, "entry count exceeds central records");
    Ok(())
}

#[test]
fn forged_central_directory_size_or_offset_is_invalid() -> Result<(), Box<dyn Error>> {
    let archive = one_entry_zip()?;

    let mut oversized = archive.bytes.clone();
    set_u32(
        &mut oversized,
        archive.eocd + 12,
        u32::try_from(archive.central_size + 1)?,
    );
    assert_invalid_structure(&oversized, "central size overlaps EOCD");

    let mut shifted = archive.bytes;
    set_u32(
        &mut shifted,
        archive.eocd + 16,
        u32::try_from(archive.central_offset + 1)?,
    );
    assert_invalid_structure(&shifted, "central offset skips its signature");
    Ok(())
}

#[test]
fn truncated_central_variable_fields_are_invalid() -> Result<(), Box<dyn Error>> {
    let archive = one_entry_zip()?;
    let central = archive.central_headers[0];
    for (field, case) in [
        (28, "truncated filename"),
        (30, "truncated extra field"),
        (32, "truncated comment"),
    ] {
        let mut bytes = archive.bytes.clone();
        set_u16(&mut bytes, central + field, u16::MAX);
        assert_invalid_structure(&bytes, case);
    }
    Ok(())
}

#[test]
fn multi_disk_markers_are_unsupported() -> Result<(), Box<dyn Error>> {
    let archive = one_entry_zip()?;

    for (field, case) in [(4, "EOCD disk"), (6, "central-directory disk")] {
        let mut bytes = archive.bytes.clone();
        set_u16(&mut bytes, archive.eocd + field, 1);
        assert_unsupported_feature(&bytes, case);
    }

    let mut entry_disk = archive.bytes;
    set_u16(&mut entry_disk, archive.central_headers[0] + 34, 1);
    assert_unsupported_feature(&entry_disk, "central entry start disk");
    Ok(())
}

#[test]
fn zip64_eocd_sentinels_are_unsupported() -> Result<(), Box<dyn Error>> {
    let archive = one_entry_zip()?;

    let mut count = archive.bytes.clone();
    set_u16(&mut count, archive.eocd + 8, u16::MAX);
    set_u16(&mut count, archive.eocd + 10, u16::MAX);
    assert_unsupported_feature(&count, "ZIP64 entry-count sentinel");

    for (field, case) in [
        (12, "ZIP64 central-size sentinel"),
        (16, "ZIP64 central-offset sentinel"),
    ] {
        let mut bytes = archive.bytes.clone();
        set_u32(&mut bytes, archive.eocd + field, u32::MAX);
        assert_unsupported_feature(&bytes, case);
    }
    Ok(())
}

#[test]
fn zip64_central_entry_sentinels_are_unsupported() -> Result<(), Box<dyn Error>> {
    let archive = one_entry_zip()?;
    let central = archive.central_headers[0];
    for (field, case) in [
        (20, "ZIP64 compressed-size sentinel"),
        (24, "ZIP64 uncompressed-size sentinel"),
        (42, "ZIP64 local-offset sentinel"),
    ] {
        let mut bytes = archive.bytes.clone();
        set_u32(&mut bytes, central + field, u32::MAX);
        assert_unsupported_feature(&bytes, case);
    }
    Ok(())
}

#[test]
fn zip64_locator_is_unsupported() -> Result<(), Box<dyn Error>> {
    let archive = one_entry_zip()?;
    let mut bytes = archive.bytes[..archive.eocd].to_vec();
    let zip64_eocd_offset = u64::try_from(bytes.len())?;

    push_u32(&mut bytes, ZIP64_EOCD_SIGNATURE);
    push_u64(&mut bytes, 44);
    push_u16(&mut bytes, ZIP64_VERSION_NEEDED);
    push_u16(&mut bytes, ZIP64_VERSION_NEEDED);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_u64(&mut bytes, 1);
    push_u64(&mut bytes, 1);
    push_u64(&mut bytes, u64::try_from(archive.central_size)?);
    push_u64(&mut bytes, u64::try_from(archive.central_offset)?);

    push_u32(&mut bytes, ZIP64_LOCATOR_SIGNATURE);
    push_u32(&mut bytes, 0);
    push_u64(&mut bytes, zip64_eocd_offset);
    push_u32(&mut bytes, 1);
    bytes.extend_from_slice(&archive.bytes[archive.eocd..]);

    assert_unsupported_feature(&bytes, "ZIP64 EOCD and locator");
    Ok(())
}

#[test]
fn encrypted_entry_flag_is_unsupported() -> Result<(), Box<dyn Error>> {
    let archive = one_entry_zip()?;
    let mut bytes = archive.bytes;
    set_u16(
        &mut bytes,
        archive.local_headers[0] + 6,
        UTF8_NAME | ENCRYPTED,
    );
    set_u16(
        &mut bytes,
        archive.central_headers[0] + 8,
        UTF8_NAME | ENCRYPTED,
    );
    assert_unsupported_feature(&bytes, "encrypted general-purpose bit");
    Ok(())
}

#[test]
fn trailing_central_records_are_invalid() -> Result<(), Box<dyn Error>> {
    let archive = stored_zip(&[(b"first.txt", b"1"), (b"second.txt", b"2")])?;
    let mut bytes = archive.bytes;
    set_u16(&mut bytes, archive.eocd + 8, 1);
    set_u16(&mut bytes, archive.eocd + 10, 1);
    assert_invalid_structure(&bytes, "undeclared second central file header");

    let archive = one_entry_zip()?;
    let mut signed = archive.bytes[..archive.eocd].to_vec();
    push_u32(&mut signed, CENTRAL_DIGITAL_SIGNATURE);
    push_u16(&mut signed, 0);
    let eocd = signed.len();
    signed.extend_from_slice(&archive.bytes[archive.eocd..]);
    set_u32(
        &mut signed,
        eocd + 12,
        u32::try_from(archive.central_size + 6)?,
    );
    assert_invalid_structure(&signed, "central-directory digital signature");
    Ok(())
}

#[test]
fn ambiguous_eocd_in_archive_comment_is_invalid() -> Result<(), Box<dyn Error>> {
    let archive = one_entry_zip()?;
    let fake_eocd = archive.bytes[archive.eocd..].to_vec();
    let mut bytes = archive.bytes;
    set_u16(
        &mut bytes,
        archive.eocd + 20,
        u16::try_from(fake_eocd.len())?,
    );
    bytes.extend_from_slice(&fake_eocd);

    assert_invalid_structure(&bytes, "second plausible EOCD in archive comment");
    Ok(())
}

fn portable_scenario() -> Result<PortableScenario, Box<dyn Error>> {
    let document: ScenarioDocument = serde_json::from_value(serde_json::json!({
        "format": "eutheto/scenario",
        "formatVersion": 1,
        "scenarioId": "018f1e2d-3c4b-7a69-8def-0123456789ab",
        "domainPack": {"id": "official.generic", "schemaVersion": 1},
        "metadata": {
            "title": "Central directory control",
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
    }))?;
    Ok(PortableScenario::current(
        Revision::new(7),
        document,
        BTreeSet::new(),
    ))
}

#[test]
fn normal_exported_scenario_bundle_still_inspects() -> Result<(), Box<dyn Error>> {
    let bundle = assemble_scenario_export(&ScenarioExportSnapshot {
        bundle_id: BundleId::from_uuid(Uuid::from_u128(0x018f_1e2d_3c4b_7a69_8def_2000_0000_0001)),
        created_at: "2026-08-29T00:00:00Z".to_owned(),
        application: ApplicationMetadata {
            name: "Eutheto".to_owned(),
            version: "0.1.0".to_owned(),
        },
        title: "Central directory control".to_owned(),
        scenario: portable_scenario()?,
        scenario_revisions: Vec::new(),
        sections: BackupSections::default(),
        nonsemantic_extensions: BTreeSet::new(),
        manifest_extensions: BTreeMap::new(),
    })?;

    let inspected = inspect(&bundle)?;
    assert_eq!(inspected.scenarios.len(), 1);
    assert_eq!(
        inspected.scenarios[0].document.metadata.title,
        "Central directory control"
    );
    Ok(())
}
