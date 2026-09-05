#![no_main]

#[path = "../../../../tests/support/portable_decode.rs"]
mod portable_decode;

use eutheto_export::PORTABLE_LIMITS;
use eutheto_import::{InspectionPolicy, MigrationRegistries, inspect_bundle};
use libfuzzer_sys::fuzz_target;

const MAX_FUZZ_ARCHIVE_BYTES: usize = 512 * 1024;

fn bounded_policy() -> InspectionPolicy {
    let mut policy = InspectionPolicy::default();
    policy.limits = PORTABLE_LIMITS;
    policy.limits.max_archive_bytes = MAX_FUZZ_ARCHIVE_BYTES as u64;
    policy.limits.max_total_uncompressed_bytes = 1024 * 1024;
    policy.limits.max_entry_bytes = 256 * 1024;
    policy.limits.max_entries = 128;
    policy.limits.max_compression_ratio = 32;
    policy.limits.max_path_bytes = 240;
    policy.limits.max_json_bytes = 256 * 1024;
    policy.limits.max_json_depth = 64;
    policy.limits.max_string_bytes = 64 * 1024;
    policy.limits.max_collection_items = 4_096;
    policy
}

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_ARCHIVE_BYTES {
        return;
    }

    let policy = bounded_policy();
    let registries = MigrationRegistries::current_only();
    let _inspection = inspect_bundle(
        data,
        &policy,
        &registries,
        &portable_decode::decode_fixture_domain,
    );
});
