use std::io;

use eutheto_solver_ortools::VerifiedWorkerArtifact;
use tauri::{AppHandle, Manager};

const RESOURCE_ROOT: &str = "solver/ortools";
const MANIFEST_SHA256: &str = env!("EUTHETO_ORTOOLS_MANIFEST_SHA256");

pub(crate) async fn load(handle: &AppHandle) -> Result<VerifiedWorkerArtifact, io::Error> {
    let resource_root = handle
        .path()
        .resource_dir()
        .map_err(|_| io::Error::other("bundled OR-Tools resource directory is unavailable"))?
        .join(RESOURCE_ROOT);
    let executable = std::env::current_exe()
        .map_err(|_| io::Error::other("application executable location is unavailable"))?;
    let executable_directory = executable
        .parent()
        .ok_or_else(|| io::Error::other("application executable has no parent directory"))?;
    let worker = executable_directory.join(worker_filename());
    let manifest_sha256 = decode_manifest_sha256()?;

    VerifiedWorkerArtifact::verify_packaged(resource_root, worker, manifest_sha256)
        .await
        .map_err(|error| io::Error::other(format!("bundled OR-Tools validation failed: {error}")))
}

fn decode_manifest_sha256() -> Result<[u8; 32], io::Error> {
    let bytes = MANIFEST_SHA256.as_bytes();
    if bytes.len() != 64 {
        return Err(io::Error::other(
            "embedded OR-Tools manifest digest has an invalid length",
        ));
    }
    let mut digest = [0_u8; 32];
    for (index, output) in digest.iter_mut().enumerate() {
        let offset = index * 2;
        *output = (hex_nibble(bytes[offset])? << 4) | hex_nibble(bytes[offset + 1])?;
    }
    Ok(digest)
}

fn hex_nibble(value: u8) -> Result<u8, io::Error> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(io::Error::other(
            "embedded OR-Tools manifest digest is not lowercase hexadecimal",
        )),
    }
}

#[cfg(windows)]
const fn worker_filename() -> &'static str {
    "ortools-worker.exe"
}

#[cfg(not(windows))]
const fn worker_filename() -> &'static str {
    "ortools-worker"
}
