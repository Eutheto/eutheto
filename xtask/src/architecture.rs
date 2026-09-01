use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const REQUIRED_PHASE02_PACKAGES: &[&str] = &[
    "eutheto-domain-api",
    "eutheto-domain-ir",
    "eutheto-planning-ir",
    "eutheto-solver-api",
    "eutheto-solver-router",
];

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Package {
    id: String,
    name: String,
    manifest_path: PathBuf,
    dependencies: Vec<Dependency>,
}

#[derive(Debug, Deserialize)]
struct Dependency {
    name: String,
    path: Option<PathBuf>,
}

pub fn verify(repo_root: &Path) -> Result<()> {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .current_dir(repo_root)
        .output()
        .context("failed to run cargo metadata for architecture verification")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed during architecture verification: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    }
    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).context("cargo metadata emitted invalid JSON")?;
    verify_metadata(repo_root, &metadata)?;
    println!("verified Phase-02 workspace dependency boundaries");
    Ok(())
}

fn verify_metadata(repo_root: &Path, metadata: &Metadata) -> Result<()> {
    let packages = workspace_packages(metadata);
    for required in REQUIRED_PHASE02_PACKAGES {
        if !packages.contains_key(required) {
            bail!("required Phase-02 workspace package `{required}` is missing")
        }
    }
    let graph = workspace_graph(&packages);

    for (name, package) in &packages {
        let relative_manifest = package
            .manifest_path
            .strip_prefix(repo_root)
            .unwrap_or(package.manifest_path.as_path());
        let is_official_domain = relative_manifest.starts_with("domains");
        if is_official_domain || *name == "eutheto-domain-api" {
            reject_reachable(
                name,
                &graph,
                |candidate| {
                    candidate.starts_with("eutheto-solver-")
                        || matches!(
                            candidate,
                            "eutheto-store"
                                | "eutheto-import"
                                | "eutheto-export"
                                | "eutheto-desktop"
                        )
                },
                "domain code must not reach backends, Tauri, persistence, or providers",
            )?;
        }
        if matches!(*name, "eutheto-domain-ir" | "eutheto-planning-ir") {
            reject_reachable(
                name,
                &graph,
                |candidate| {
                    candidate.starts_with("eutheto-solver-")
                        || matches!(
                            candidate,
                            "eutheto-domain-api"
                                | "eutheto-command"
                                | "eutheto-core"
                                | "eutheto-store"
                                | "eutheto-import"
                                | "eutheto-export"
                                | "eutheto-desktop"
                        )
                },
                "planning contracts must remain independent of packs, adapters, and infrastructure",
            )?;
        }
        if name.starts_with("eutheto-solver-") {
            reject_reachable(
                name,
                &graph,
                |candidate| {
                    matches!(
                        candidate,
                        "eutheto-domain-api"
                            | "eutheto-command"
                            | "eutheto-core"
                            | "eutheto-store"
                            | "eutheto-import"
                            | "eutheto-export"
                            | "eutheto-desktop"
                    ) || packages.get(candidate).is_some_and(|candidate_package| {
                        candidate_package
                            .manifest_path
                            .strip_prefix(repo_root)
                            .unwrap_or(candidate_package.manifest_path.as_path())
                            .starts_with("domains")
                    })
                },
                "solver code must not reach official domains, application services, or infrastructure",
            )?;
        }
    }

    Ok(())
}

fn workspace_packages(metadata: &Metadata) -> BTreeMap<&str, &Package> {
    let workspace_ids = metadata
        .workspace_members
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    metadata
        .packages
        .iter()
        .filter(|package| workspace_ids.contains(package.id.as_str()))
        .map(|package| (package.name.as_str(), package))
        .collect()
}

fn workspace_graph<'a>(
    packages: &BTreeMap<&'a str, &'a Package>,
) -> BTreeMap<&'a str, BTreeSet<&'a str>> {
    packages
        .iter()
        .map(|(name, package)| {
            let dependencies = package
                .dependencies
                .iter()
                .filter(|dependency| dependency.path.is_some())
                .filter_map(|dependency| {
                    packages
                        .contains_key(dependency.name.as_str())
                        .then_some(dependency.name.as_str())
                })
                .collect();
            (*name, dependencies)
        })
        .collect()
}

fn reject_reachable<'a>(
    root: &'a str,
    graph: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    forbidden: impl Fn(&str) -> bool,
    rule: &str,
) -> Result<()> {
    let mut pending = graph
        .get(root)
        .into_iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    let mut visited = BTreeSet::new();
    while let Some(candidate) = pending.pop() {
        if !visited.insert(candidate) {
            continue;
        }
        if forbidden(candidate) {
            bail!("forbidden dependency path from `{root}` to `{candidate}`: {rule}")
        }
        if let Some(next) = graph.get(candidate) {
            pending.extend(next.iter().copied());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use anyhow::Result;

    use super::reject_reachable;

    #[test]
    fn transitive_forbidden_edges_are_rejected() {
        let graph = BTreeMap::from([
            ("domain", BTreeSet::from(["middle"])),
            ("middle", BTreeSet::from(["solver"])),
            ("solver", BTreeSet::new()),
        ]);
        let result = reject_reachable(
            "domain",
            &graph,
            |candidate| candidate == "solver",
            "test boundary",
        );
        assert!(result.is_err());
    }

    #[test]
    fn inward_only_edges_are_accepted() -> Result<()> {
        let graph = BTreeMap::from([
            ("solver", BTreeSet::from(["planning"])),
            ("planning", BTreeSet::from(["types"])),
            ("types", BTreeSet::new()),
        ]);
        reject_reachable(
            "solver",
            &graph,
            |candidate| candidate == "domain",
            "test boundary",
        )
    }
}
