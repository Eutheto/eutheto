use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::{generate, protocol, supply_chain};

const REQUIRED_TRACKED_INPUTS_AND_OUTPUTS: &[&str] = &[
    "Cargo.lock",
    "pnpm-lock.yaml",
    "protocol/solver-worker.proto",
    "protocol/version.json",
    "crates/eutheto-protocol/src/generated/eutheto.worker.v1.rs",
    "protocol/generated/cpp/protocol-policy.h",
    "protocol/generated/cpp/solver-worker.pb.cc",
    "protocol/generated/cpp/solver-worker.pb.h",
    "protocol/generated/eutheto.worker.v1.descriptor.pb",
    "protocol/golden/finished.frame.hex",
    "protocol/golden/finished.json",
    "protocol/golden/handshake-error.frame.hex",
    "protocol/golden/handshake-error.json",
    "protocol/golden/handshake-request.frame.hex",
    "protocol/golden/handshake-request.json",
    "protocol/golden/handshake-response.frame.hex",
    "protocol/golden/handshake-response.json",
    "protocol/golden/incumbent.frame.hex",
    "protocol/golden/incumbent.json",
    "protocol/golden/progress.frame.hex",
    "protocol/golden/progress.json",
    "protocol/golden/solve-request.frame.hex",
    "protocol/golden/solve-request.json",
    "protocol/golden/started.frame.hex",
    "protocol/golden/started.json",
    "protocol/golden/worker-error.frame.hex",
    "protocol/golden/worker-error.json",
    "xtask/supply-chain-inputs.json",
    "THIRD_PARTY_NOTICES.md",
    "xtask/generated/license-inventory.json",
    "xtask/generated/sbom.spdx.json",
];

pub fn verify_clean(repo_root: &Path) -> Result<()> {
    verify_tracked_tree_clean(repo_root)?;
    verify_required_paths_tracked(repo_root)?;
    generate::check(repo_root).context("generated source drift check failed")?;
    protocol::verify(repo_root).context("worker protocol verification failed")?;
    supply_chain::check_licenses(repo_root).context("license artifact drift check failed")?;
    supply_chain::check_sbom(repo_root).context("SBOM drift check failed")?;
    println!("verified tracked tree and all generated artifacts are clean");
    Ok(())
}

pub fn assemble_manifest() -> Result<()> {
    bail!(
        "release assemble-manifest is unavailable until Phase 11 supplies finalized product identity, target artifacts, exact artifact digests, and protected build/sign evidence"
    )
}

fn verify_tracked_tree_clean(repo_root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=no"])
        .current_dir(repo_root)
        .output()
        .context("failed to execute git for tracked-tree verification")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git could not verify tracked-tree cleanliness (status {}): {}",
            output.status,
            stderr.trim()
        );
    }
    if !output.stdout.is_empty() {
        let changed = String::from_utf8_lossy(&output.stdout);
        bail!(
            "tracked tree is not clean; commit or restore these tracked changes before release preflight:\n{}",
            changed.trim_end()
        );
    }
    Ok(())
}

fn verify_required_paths_tracked(repo_root: &Path) -> Result<()> {
    for path in REQUIRED_TRACKED_INPUTS_AND_OUTPUTS {
        let output = Command::new("git")
            .args(["ls-files", "--error-unmatch", "--", path])
            .current_dir(repo_root)
            .output()
            .with_context(|| format!("failed to ask git whether `{path}` is tracked"))?;
        if !output.status.success() {
            bail!("release preflight requires `{path}` to exist in the Git index");
        }
    }
    Ok(())
}
