use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Write as _};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde::{Deserialize, Serialize};

const CARGO_LOCK: &str = "Cargo.lock";
const PNPM_LOCK: &str = "pnpm-lock.yaml";
const REVIEWED_INPUTS: &str = "xtask/supply-chain-inputs.json";
const LICENSE_INVENTORY: &str = "xtask/generated/license-inventory.json";
const NOTICE: &str = "THIRD_PARTY_NOTICES.md";
const SBOM: &str = "xtask/generated/sbom.spdx.json";
const NOASSERTION: &str = "NOASSERTION";

pub type SupplyChainResult<T> = Result<T, SupplyChainError>;

#[derive(Debug)]
pub enum SupplyChainError {
    MissingInput {
        path: PathBuf,
    },
    ReadInput {
        path: PathBuf,
        source: io::Error,
    },
    MalformedLock {
        path: PathBuf,
        detail: String,
    },
    MalformedReviewedInput {
        path: PathBuf,
        detail: String,
    },
    StaleReviewedInput {
        detail: String,
    },
    Render {
        artifact: &'static str,
        detail: String,
    },
    WriteArtifact {
        path: PathBuf,
        source: io::Error,
    },
    Drift {
        paths: Vec<&'static str>,
    },
}

impl fmt::Display for SupplyChainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingInput { path } => {
                write!(
                    formatter,
                    "required supply-chain input is missing: {}",
                    path.display()
                )
            }
            Self::ReadInput { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::MalformedLock { path, detail } => {
                write!(
                    formatter,
                    "malformed lock file {}: {detail}",
                    path.display()
                )
            }
            Self::MalformedReviewedInput { path, detail } => {
                write!(
                    formatter,
                    "malformed reviewed input {}: {detail}",
                    path.display()
                )
            }
            Self::StaleReviewedInput { detail } => {
                write!(
                    formatter,
                    "reviewed supply-chain input does not match the locks: {detail}"
                )
            }
            Self::Render { artifact, detail } => {
                write!(formatter, "failed to render {artifact}: {detail}")
            }
            Self::WriteArtifact { path, source } => {
                write!(formatter, "failed to write {}: {source}", path.display())
            }
            Self::Drift { paths } => write!(
                formatter,
                "supply-chain artifacts are missing or stale: {}; run `cargo xtask licenses generate` and `cargo xtask sbom generate`",
                paths.join(", ")
            ),
        }
    }
}

impl Error for SupplyChainError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ReadInput { source, .. } | Self::WriteArtifact { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewedInputs {
    schema_version: u32,
    document_name: String,
    document_namespace: String,
    created: String,
    workspace_packages: Vec<ReviewedWorkspacePackage>,
    license_conclusions: Vec<ReviewedLicense>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewedWorkspacePackage {
    ecosystem: String,
    name: String,
    version: String,
    path: String,
    license: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewedLicense {
    ecosystem: String,
    name: String,
    version: String,
    license: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PackageKey {
    ecosystem: String,
    name: String,
    version: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PnpmPackage {
    key: PackageKey,
    checksum: PackageChecksum,
    source: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PackageChecksum {
    algorithm: ChecksumAlgorithm,
    value: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
enum ChecksumAlgorithm {
    #[serde(rename = "SHA256")]
    Sha256,
    #[serde(rename = "SHA512")]
    Sha512,
}

#[derive(Clone, Debug)]
struct CargoPackage {
    key: PackageKey,
    source: Option<String>,
    checksum: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InventoryPackage {
    ecosystem: String,
    name: String,
    version: String,
    kind: String,
    license_concluded: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksum: Option<PackageChecksum>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LicenseInventory<'a> {
    schema_version: u32,
    generated_by: &'static str,
    authoritative_inputs: [&'static str; 3],
    packages: &'a [InventoryPackage],
}

#[derive(Debug)]
struct Inventory {
    reviewed: ReviewedInputs,
    packages: Vec<InventoryPackage>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpdxDocument<'a> {
    spdx_version: &'static str,
    data_license: &'static str,
    #[serde(rename = "SPDXID")]
    spdx_id: &'static str,
    name: &'a str,
    document_namespace: &'a str,
    creation_info: SpdxCreationInfo<'a>,
    packages: Vec<SpdxPackage>,
    relationships: Vec<SpdxRelationship>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpdxCreationInfo<'a> {
    created: &'a str,
    creators: [&'static str; 1],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpdxPackage {
    #[serde(rename = "SPDXID")]
    spdx_id: String,
    name: String,
    version_info: String,
    download_location: String,
    files_analyzed: bool,
    license_concluded: String,
    license_declared: String,
    copyright_text: &'static str,
    external_refs: [SpdxExternalRef; 1],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    checksums: Vec<SpdxChecksum>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpdxExternalRef {
    #[serde(rename = "referenceCategory")]
    category: &'static str,
    #[serde(rename = "referenceType")]
    kind: &'static str,
    #[serde(rename = "referenceLocator")]
    locator: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpdxChecksum {
    algorithm: &'static str,
    checksum_value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpdxRelationship {
    spdx_element_id: &'static str,
    relationship_type: &'static str,
    related_spdx_element: String,
}

pub fn generate_licenses(repo_root: &Path) -> SupplyChainResult<()> {
    let inventory = load_inventory(repo_root)?;
    let notice = render_notice(&inventory.packages)?;
    let inventory_json = render_inventory(&inventory.packages)?;
    write_if_changed(repo_root, NOTICE, &notice)?;
    write_if_changed(repo_root, LICENSE_INVENTORY, &inventory_json)?;
    println!(
        "generated license inventory for {} workspace package(s) and {} locked dependency package(s)",
        inventory
            .packages
            .iter()
            .filter(|package| package.kind == "workspace")
            .count(),
        inventory
            .packages
            .iter()
            .filter(|package| package.kind == "dependency")
            .count()
    );
    Ok(())
}

pub fn generate_sbom(repo_root: &Path) -> SupplyChainResult<()> {
    let inventory = load_inventory(repo_root)?;
    let sbom = render_sbom(&inventory)?;
    write_if_changed(repo_root, SBOM, &sbom)?;
    println!(
        "generated SPDX-JSON SBOM for {} package(s)",
        inventory.packages.len()
    );
    Ok(())
}

pub fn check_licenses(repo_root: &Path) -> SupplyChainResult<()> {
    let inventory = load_inventory(repo_root)?;
    let expected_notice = render_notice(&inventory.packages)?;
    let expected_inventory = render_inventory(&inventory.packages)?;
    check_outputs(
        repo_root,
        &[
            (NOTICE, expected_notice),
            (LICENSE_INVENTORY, expected_inventory),
        ],
    )
}

pub fn check_sbom(repo_root: &Path) -> SupplyChainResult<()> {
    let inventory = load_inventory(repo_root)?;
    let expected = render_sbom(&inventory)?;
    check_outputs(repo_root, &[(SBOM, expected)])
}

#[allow(
    clippy::too_many_lines,
    reason = "inventory reconciliation is one order-sensitive validation pass across both lockfiles"
)]
fn load_inventory(repo_root: &Path) -> SupplyChainResult<Inventory> {
    let reviewed = read_reviewed_inputs(repo_root)?;
    let cargo_packages = parse_cargo_lock(repo_root)?;
    let (pnpm_packages, pnpm_importers) = parse_pnpm_lock(repo_root)?;

    let mut workspace = BTreeMap::new();
    for package in &reviewed.workspace_packages {
        validate_reviewed_workspace(package, repo_root)?;
        let key = PackageKey {
            ecosystem: package.ecosystem.clone(),
            name: package.name.clone(),
            version: package.version.clone(),
        };
        if workspace.insert(key, package).is_some() {
            return Err(SupplyChainError::StaleReviewedInput {
                detail: format!(
                    "duplicate workspace package {}:{}@{}",
                    package.ecosystem, package.name, package.version
                ),
            });
        }
    }

    let mut conclusions = BTreeMap::new();
    for conclusion in &reviewed.license_conclusions {
        validate_ecosystem(&conclusion.ecosystem, "license conclusion")?;
        require_nonempty(&conclusion.name, "license conclusion name")?;
        require_nonempty(&conclusion.version, "license conclusion version")?;
        require_nonempty(&conclusion.license, "license conclusion SPDX expression")?;
        let key = PackageKey {
            ecosystem: conclusion.ecosystem.clone(),
            name: conclusion.name.clone(),
            version: conclusion.version.clone(),
        };
        if conclusions
            .insert(key, conclusion.license.clone())
            .is_some()
        {
            return Err(SupplyChainError::StaleReviewedInput {
                detail: format!(
                    "duplicate license conclusion {}:{}@{}",
                    conclusion.ecosystem, conclusion.name, conclusion.version
                ),
            });
        }
    }

    let mut packages = BTreeMap::<PackageKey, InventoryPackage>::new();
    let mut seen_workspace = BTreeSet::new();
    for package in cargo_packages {
        let (kind, license, source) = match &package.source {
            None => {
                let reviewed_package = workspace.get(&package.key).ok_or_else(|| {
                    SupplyChainError::StaleReviewedInput {
                        detail: format!(
                            "Cargo workspace package {}@{} is absent from {REVIEWED_INPUTS}",
                            package.key.name, package.key.version
                        ),
                    }
                })?;
                seen_workspace.insert(package.key.clone());
                (
                    "workspace".to_owned(),
                    reviewed_package.license.clone(),
                    reviewed_package.path.clone(),
                )
            }
            Some(source) => (
                "dependency".to_owned(),
                conclusions
                    .get(&package.key)
                    .cloned()
                    .unwrap_or_else(|| NOASSERTION.to_owned()),
                source.clone(),
            ),
        };
        insert_package(
            &mut packages,
            InventoryPackage {
                ecosystem: package.key.ecosystem.clone(),
                name: package.key.name.clone(),
                version: package.key.version.clone(),
                kind,
                license_concluded: license,
                source,
                checksum: package.checksum.map(|value| PackageChecksum {
                    algorithm: ChecksumAlgorithm::Sha256,
                    value,
                }),
            },
        )?;
    }

    for package in workspace
        .values()
        .filter(|package| package.ecosystem == "npm")
    {
        if !pnpm_importers.contains(&package.path) {
            return Err(SupplyChainError::StaleReviewedInput {
                detail: format!(
                    "npm workspace package {} points to importer `{}` absent from {PNPM_LOCK}",
                    package.name, package.path
                ),
            });
        }
        let key = PackageKey {
            ecosystem: package.ecosystem.clone(),
            name: package.name.clone(),
            version: package.version.clone(),
        };
        seen_workspace.insert(key.clone());
        insert_package(
            &mut packages,
            InventoryPackage {
                ecosystem: key.ecosystem,
                name: key.name,
                version: key.version,
                kind: "workspace".to_owned(),
                license_concluded: package.license.clone(),
                source: package.path.clone(),
                checksum: None,
            },
        )?;
    }

    let reviewed_npm_paths: BTreeSet<&str> = workspace
        .values()
        .filter(|package| package.ecosystem == "npm")
        .map(|package| package.path.as_str())
        .collect();
    for importer in &pnpm_importers {
        if !reviewed_npm_paths.contains(importer.as_str()) {
            return Err(SupplyChainError::StaleReviewedInput {
                detail: format!(
                    "pnpm importer `{importer}` is absent from the reviewed workspace package list"
                ),
            });
        }
    }

    for package in pnpm_packages {
        insert_package(
            &mut packages,
            InventoryPackage {
                ecosystem: package.key.ecosystem.clone(),
                name: package.key.name.clone(),
                version: package.key.version.clone(),
                kind: "dependency".to_owned(),
                license_concluded: conclusions
                    .get(&package.key)
                    .cloned()
                    .unwrap_or_else(|| NOASSERTION.to_owned()),
                source: package.source,
                checksum: Some(package.checksum),
            },
        )?;
    }

    for key in workspace.keys() {
        if !seen_workspace.contains(key) {
            return Err(SupplyChainError::StaleReviewedInput {
                detail: format!(
                    "reviewed workspace package {}:{}@{} is absent from its committed lock",
                    key.ecosystem, key.name, key.version
                ),
            });
        }
    }
    for key in conclusions.keys() {
        let is_dependency = packages
            .get(key)
            .is_some_and(|package| package.kind == "dependency");
        if !is_dependency {
            return Err(SupplyChainError::StaleReviewedInput {
                detail: format!(
                    "license conclusion {}:{}@{} does not match a locked dependency",
                    key.ecosystem, key.name, key.version
                ),
            });
        }
    }

    Ok(Inventory {
        reviewed,
        packages: packages.into_values().collect(),
    })
}

fn read_reviewed_inputs(repo_root: &Path) -> SupplyChainResult<ReviewedInputs> {
    let path = repo_root.join(REVIEWED_INPUTS);
    let contents = read_required(&path)?;
    let inputs: ReviewedInputs = serde_json::from_str(&contents).map_err(|error| {
        SupplyChainError::MalformedReviewedInput {
            path: path.clone(),
            detail: error.to_string(),
        }
    })?;
    if inputs.schema_version != 1 {
        return Err(SupplyChainError::MalformedReviewedInput {
            path,
            detail: format!(
                "unsupported schemaVersion {}; expected 1",
                inputs.schema_version
            ),
        });
    }
    require_nonempty(&inputs.document_name, "documentName")?;
    if !inputs.document_namespace.starts_with("urn:")
        && !inputs.document_namespace.starts_with("https://")
    {
        return Err(SupplyChainError::MalformedReviewedInput {
            path: repo_root.join(REVIEWED_INPUTS),
            detail: "documentNamespace must be an absolute urn: or https: URI".to_owned(),
        });
    }
    if !looks_like_utc_timestamp(&inputs.created) {
        return Err(SupplyChainError::MalformedReviewedInput {
            path: repo_root.join(REVIEWED_INPUTS),
            detail: "created must be a fixed UTC timestamp in YYYY-MM-DDTHH:MM:SSZ form".to_owned(),
        });
    }
    Ok(inputs)
}

fn validate_reviewed_workspace(
    package: &ReviewedWorkspacePackage,
    repo_root: &Path,
) -> SupplyChainResult<()> {
    validate_ecosystem(&package.ecosystem, "workspace package")?;
    require_nonempty(&package.name, "workspace package name")?;
    require_nonempty(&package.version, "workspace package version")?;
    require_nonempty(&package.license, "workspace package SPDX license")?;
    if package.path.is_empty()
        || package.path.starts_with('/')
        || package.path.split('/').any(|component| component == "..")
    {
        return Err(SupplyChainError::MalformedReviewedInput {
            path: repo_root.join(REVIEWED_INPUTS),
            detail: format!(
                "workspace package path is not repository-relative: {}",
                package.path
            ),
        });
    }
    Ok(())
}

fn validate_ecosystem(ecosystem: &str, subject: &str) -> SupplyChainResult<()> {
    if matches!(ecosystem, "cargo" | "npm") {
        Ok(())
    } else {
        Err(SupplyChainError::StaleReviewedInput {
            detail: format!("{subject} has unsupported ecosystem `{ecosystem}`"),
        })
    }
}

fn require_nonempty(value: &str, field: &str) -> SupplyChainResult<()> {
    if value.trim().is_empty() {
        Err(SupplyChainError::StaleReviewedInput {
            detail: format!("{field} must not be empty"),
        })
    } else {
        Ok(())
    }
}

fn parse_cargo_lock(repo_root: &Path) -> SupplyChainResult<Vec<CargoPackage>> {
    let path = repo_root.join(CARGO_LOCK);
    let contents = read_required(&path)?;
    let mut lock_version = None;
    let mut packages = Vec::new();
    let mut current = None;

    for (index, raw_line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[package]]" {
            if let Some(package) = current.take() {
                packages.push(finish_cargo_package(package, &path, line_number)?);
            }
            current = Some(PartialCargoPackage::default());
            continue;
        }
        if line.starts_with("[[package") {
            return malformed_lock(
                &path,
                format!("line {line_number}: malformed package header"),
            );
        }
        if current.is_none() && line.starts_with("version") {
            let value = assignment_value(line, "version", &path, line_number)?;
            let parsed = value
                .parse::<u32>()
                .map_err(|_| SupplyChainError::MalformedLock {
                    path: path.clone(),
                    detail: format!("line {line_number}: lock version is not an integer"),
                })?;
            if lock_version.replace(parsed).is_some() {
                return malformed_lock(
                    &path,
                    format!("line {line_number}: duplicate top-level lock version"),
                );
            }
            continue;
        }
        if let Some(package) = current.as_mut() {
            if line.starts_with("name") {
                package.name = Some(parse_toml_string(
                    assignment_value(line, "name", &path, line_number)?,
                    &path,
                    line_number,
                )?);
            } else if line.starts_with("version") {
                package.version = Some(parse_toml_string(
                    assignment_value(line, "version", &path, line_number)?,
                    &path,
                    line_number,
                )?);
            } else if line.starts_with("source") {
                package.source = Some(parse_toml_string(
                    assignment_value(line, "source", &path, line_number)?,
                    &path,
                    line_number,
                )?);
            } else if line.starts_with("checksum") {
                let checksum = parse_toml_string(
                    assignment_value(line, "checksum", &path, line_number)?,
                    &path,
                    line_number,
                )?;
                if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return malformed_lock(
                        &path,
                        format!("line {line_number}: checksum is not a 64-digit SHA-256 value"),
                    );
                }
                package.checksum = Some(checksum.to_ascii_lowercase());
            }
        }
    }
    if let Some(package) = current {
        packages.push(finish_cargo_package(
            package,
            &path,
            contents.lines().count() + 1,
        )?);
    }

    match lock_version {
        None => return malformed_lock(&path, "missing top-level lock version".to_owned()),
        Some(4) => {}
        Some(version) => {
            return malformed_lock(
                &path,
                format!("unsupported Cargo lock version `{version}`; expected 4"),
            );
        }
    }
    if packages.is_empty() {
        return malformed_lock(&path, "no [[package]] entries".to_owned());
    }
    packages.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(packages)
}

#[derive(Default)]
struct PartialCargoPackage {
    name: Option<String>,
    version: Option<String>,
    source: Option<String>,
    checksum: Option<String>,
}

fn finish_cargo_package(
    package: PartialCargoPackage,
    path: &Path,
    line_number: usize,
) -> SupplyChainResult<CargoPackage> {
    let name = package
        .name
        .ok_or_else(|| SupplyChainError::MalformedLock {
            path: path.to_path_buf(),
            detail: format!("package ending before line {line_number} has no name"),
        })?;
    let version = package
        .version
        .ok_or_else(|| SupplyChainError::MalformedLock {
            path: path.to_path_buf(),
            detail: format!("package `{name}` ending before line {line_number} has no version"),
        })?;
    Ok(CargoPackage {
        key: PackageKey {
            ecosystem: "cargo".to_owned(),
            name,
            version,
        },
        source: package.source,
        checksum: package.checksum,
    })
}

fn assignment_value<'a>(
    line: &'a str,
    key: &str,
    path: &Path,
    line_number: usize,
) -> SupplyChainResult<&'a str> {
    let (actual_key, value) =
        line.split_once('=')
            .ok_or_else(|| SupplyChainError::MalformedLock {
                path: path.to_path_buf(),
                detail: format!("line {line_number}: expected `{key} = value`"),
            })?;
    if actual_key.trim() != key || value.trim().is_empty() {
        return malformed_lock(
            path,
            format!("line {line_number}: expected `{key} = value`"),
        );
    }
    Ok(value.trim())
}

fn parse_toml_string(value: &str, path: &Path, line_number: usize) -> SupplyChainResult<String> {
    serde_json::from_str::<String>(value).map_err(|error| SupplyChainError::MalformedLock {
        path: path.to_path_buf(),
        detail: format!("line {line_number}: invalid quoted string: {error}"),
    })
}

fn parse_pnpm_lock(repo_root: &Path) -> SupplyChainResult<(Vec<PnpmPackage>, BTreeSet<String>)> {
    let path = repo_root.join(PNPM_LOCK);
    let contents = read_required(&path)?;
    if contents.lines().any(|line| line.contains('\t')) {
        return malformed_lock(
            &path,
            "tab indentation is not valid in the committed pnpm lock".to_owned(),
        );
    }

    let lock_version = contents
        .lines()
        .find_map(|line| line.strip_prefix("lockfileVersion:"))
        .map(str::trim)
        .ok_or_else(|| SupplyChainError::MalformedLock {
            path: path.clone(),
            detail: "missing top-level lockfileVersion".to_owned(),
        })?;
    let lock_version = parse_yaml_scalar(lock_version, &path, 1)?;
    if lock_version != "9.0" {
        return malformed_lock(
            &path,
            format!("unsupported lockfileVersion `{lock_version}`; pnpm 11 must emit 9.0"),
        );
    }

    let importers = parse_yaml_section_keys(&contents, "importers", &path)?;
    if importers.is_empty() {
        return malformed_lock(&path, "the importers section is empty".to_owned());
    }
    let package_entries = parse_yaml_section_keys(&contents, "packages", &path)?;
    if package_entries.is_empty() {
        return malformed_lock(&path, "the packages section is empty".to_owned());
    }

    let packages = parse_pnpm_packages(&contents, package_entries, &path)?;
    Ok((packages, importers.into_keys().collect()))
}

fn parse_pnpm_packages(
    contents: &str,
    package_entries: BTreeMap<String, usize>,
    path: &Path,
) -> SupplyChainResult<Vec<PnpmPackage>> {
    let lines: Vec<_> = contents.lines().collect();
    let mut entries: Vec<_> = package_entries.into_iter().collect();
    entries.sort_by_key(|(_, line_number)| *line_number);
    let section_end = entries.last().map_or(lines.len() + 1, |(_, line_number)| {
        lines
            .iter()
            .enumerate()
            .skip(*line_number)
            .find(|(_, line)| !line.is_empty() && !line.starts_with(' '))
            .map_or(lines.len() + 1, |(index, _)| index + 1)
    });
    let mut packages = BTreeMap::new();

    for (index, (entry, line_number)) in entries.iter().enumerate() {
        let end_line = entries
            .get(index + 1)
            .map_or(section_end, |(_, next_line)| *next_line);
        let key = parse_pnpm_package_key(entry, path, *line_number)?;
        let (checksum, tarball) =
            parse_pnpm_resolution(&lines, *line_number, end_line, entry, path)?;
        let source = tarball.unwrap_or_else(|| canonical_npm_tarball_url(&key.name, &key.version));
        let package = PnpmPackage {
            key: key.clone(),
            checksum,
            source,
        };
        if packages.insert(key.clone(), package).is_some() {
            return malformed_lock(
                path,
                format!(
                    "line {line_number}: duplicate package identity npm:{}@{}",
                    key.name, key.version
                ),
            );
        }
    }

    Ok(packages.into_values().collect())
}

fn parse_pnpm_resolution(
    lines: &[&str],
    package_line: usize,
    end_line: usize,
    entry: &str,
    path: &Path,
) -> SupplyChainResult<(PackageChecksum, Option<String>)> {
    let mut resolution = None;
    for (index, line) in lines
        .iter()
        .enumerate()
        .take(end_line - 1)
        .skip(package_line)
    {
        let line_number = index + 1;
        let line = *line;
        let Some(value) = line.strip_prefix("    resolution:") else {
            continue;
        };
        if resolution.is_some() {
            return malformed_lock(
                path,
                format!("line {line_number}: package `{entry}` has duplicate resolution"),
            );
        }
        resolution = Some(parse_pnpm_resolution_value(
            lines,
            line_number,
            end_line,
            value.trim(),
            path,
        )?);
    }

    resolution.ok_or_else(|| SupplyChainError::MalformedLock {
        path: path.to_path_buf(),
        detail: format!("line {package_line}: package `{entry}` is missing resolution"),
    })
}

fn parse_pnpm_resolution_value(
    lines: &[&str],
    resolution_line: usize,
    end_line: usize,
    value: &str,
    path: &Path,
) -> SupplyChainResult<(PackageChecksum, Option<String>)> {
    let mut integrity = None;
    let mut tarball = None;
    if value.is_empty() {
        for (index, line) in lines
            .iter()
            .enumerate()
            .take(end_line - 1)
            .skip(resolution_line)
        {
            let line_number = index + 1;
            let line = *line;
            if !line.starts_with("      ") {
                break;
            }
            let Some((key, value)) = line[6..].split_once(':') else {
                continue;
            };
            parse_pnpm_resolution_field(
                key.trim(),
                value.trim(),
                line_number,
                path,
                &mut integrity,
                &mut tarball,
            )?;
        }
    } else {
        let mapping = value
            .strip_prefix('{')
            .and_then(|value| value.strip_suffix('}'))
            .ok_or_else(|| SupplyChainError::MalformedLock {
                path: path.to_path_buf(),
                detail: format!("line {resolution_line}: resolution must be a mapping"),
            })?;
        for field in split_yaml_flow_fields(mapping, path, resolution_line)? {
            let (key, value) =
                field
                    .split_once(':')
                    .ok_or_else(|| SupplyChainError::MalformedLock {
                        path: path.to_path_buf(),
                        detail: format!(
                            "line {resolution_line}: malformed field in resolution mapping"
                        ),
                    })?;
            parse_pnpm_resolution_field(
                key.trim(),
                value.trim(),
                resolution_line,
                path,
                &mut integrity,
                &mut tarball,
            )?;
        }
    }

    let (integrity, integrity_line) = integrity.ok_or_else(|| SupplyChainError::MalformedLock {
        path: path.to_path_buf(),
        detail: format!("line {resolution_line}: resolution is missing integrity"),
    })?;
    Ok((
        decode_sha512_integrity(&integrity, path, integrity_line)?,
        tarball.map(|(value, _)| value),
    ))
}

fn parse_pnpm_resolution_field(
    key: &str,
    value: &str,
    line_number: usize,
    path: &Path,
    integrity: &mut Option<(String, usize)>,
    tarball: &mut Option<(String, usize)>,
) -> SupplyChainResult<()> {
    let destination = match key {
        "integrity" => integrity,
        "tarball" => tarball,
        _ => return Ok(()),
    };
    let value = parse_yaml_scalar(value, path, line_number)?;
    if destination.replace((value, line_number)).is_some() {
        return malformed_lock(
            path,
            format!("line {line_number}: duplicate resolution `{key}` field"),
        );
    }
    Ok(())
}

fn split_yaml_flow_fields<'a>(
    mapping: &'a str,
    path: &Path,
    line_number: usize,
) -> SupplyChainResult<Vec<&'a str>> {
    let mut fields = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    for (offset, character) in mapping.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if character == '\\' => escaped = true,
            Some(current) if character == current => quote = None,
            None if matches!(character, '"' | '\'') => quote = Some(character),
            None if character == ',' => {
                fields.push(mapping[start..offset].trim());
                start = offset + 1;
            }
            Some(_) | None => {}
        }
    }
    if quote.is_some() {
        return malformed_lock(
            path,
            format!("line {line_number}: unterminated quote in resolution mapping"),
        );
    }
    fields.push(mapping[start..].trim());
    if fields.iter().any(|field| field.is_empty()) {
        return malformed_lock(
            path,
            format!("line {line_number}: empty field in resolution mapping"),
        );
    }
    Ok(fields)
}

fn decode_sha512_integrity(
    integrity: &str,
    path: &Path,
    line_number: usize,
) -> SupplyChainResult<PackageChecksum> {
    let (algorithm, encoded) =
        integrity
            .split_once('-')
            .ok_or_else(|| SupplyChainError::MalformedLock {
                path: path.to_path_buf(),
                detail: format!("line {line_number}: malformed package integrity"),
            })?;
    if algorithm != "sha512" {
        return malformed_lock(
            path,
            format!("line {line_number}: unsupported package integrity algorithm `{algorithm}`"),
        );
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| SupplyChainError::MalformedLock {
            path: path.to_path_buf(),
            detail: format!("line {line_number}: malformed SHA-512 integrity: {error}"),
        })?;
    if decoded.len() != 64 {
        return malformed_lock(
            path,
            format!(
                "line {line_number}: SHA-512 integrity decoded to {} bytes; expected 64",
                decoded.len()
            ),
        );
    }
    let mut value = String::with_capacity(128);
    for byte in decoded {
        let _ = write!(value, "{byte:02x}");
    }
    Ok(PackageChecksum {
        algorithm: ChecksumAlgorithm::Sha512,
        value,
    })
}

fn parse_yaml_section_keys(
    contents: &str,
    section: &str,
    path: &Path,
) -> SupplyChainResult<BTreeMap<String, usize>> {
    let header = format!("{section}:");
    let mut in_section = false;
    let mut found = false;
    let mut keys = BTreeMap::new();
    for (index, line) in contents.lines().enumerate() {
        let line_number = index + 1;
        if !in_section {
            if line == header {
                in_section = true;
                found = true;
            }
            continue;
        }
        if !line.is_empty() && !line.starts_with(' ') {
            break;
        }
        if let Some(entry) = line.strip_prefix("  ") {
            if entry.starts_with(' ')
                || entry.trim().is_empty()
                || entry.trim_start().starts_with('#')
            {
                continue;
            }

            let mut characters = entry.char_indices().peekable();
            let mut quote = None;
            let mut escaped = false;
            let mut separator = None;
            while let Some((offset, character)) = characters.next() {
                match quote {
                    Some('"') => {
                        if escaped {
                            escaped = false;
                        } else if character == '\\' {
                            escaped = true;
                        } else if character == '"' {
                            quote = None;
                        }
                    }
                    Some('\'') => {
                        if character == '\'' {
                            if characters.peek().is_some_and(|(_, next)| *next == '\'') {
                                characters.next();
                            } else {
                                quote = None;
                            }
                        }
                    }
                    Some(_) => unreachable!("only YAML string quote characters are tracked"),
                    None if offset == 0 && matches!(character, '"' | '\'') => {
                        quote = Some(character);
                    }
                    None if character == ':' => {
                        let inline_value = &entry[offset + character.len_utf8()..];
                        if inline_value.is_empty() || inline_value.starts_with(' ') {
                            separator = Some((offset, inline_value));
                            break;
                        }
                    }
                    None => {}
                }
            }
            let (separator, inline_value) =
                separator.ok_or_else(|| SupplyChainError::MalformedLock {
                    path: path.to_path_buf(),
                    detail: format!("line {line_number}: malformed key in `{section}` section"),
                })?;
            if !matches!(inline_value, "" | " {}") {
                return malformed_lock(
                    path,
                    format!("line {line_number}: malformed key in `{section}` section"),
                );
            }

            let key = parse_yaml_scalar(entry[..separator].trim(), path, line_number)?;
            if keys.insert(key.clone(), line_number).is_some() {
                return malformed_lock(
                    path,
                    format!("line {line_number}: duplicate `{section}` key `{key}`"),
                );
            }
        }
    }
    if !found {
        return malformed_lock(path, format!("missing top-level `{section}` section"));
    }
    Ok(keys)
}

fn parse_yaml_scalar(value: &str, path: &Path, line_number: usize) -> SupplyChainResult<String> {
    if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        return Ok(value[1..value.len() - 1].replace("''", "'"));
    }
    if value.starts_with('"') {
        return serde_json::from_str(value).map_err(|error| SupplyChainError::MalformedLock {
            path: path.to_path_buf(),
            detail: format!("line {line_number}: invalid quoted YAML scalar: {error}"),
        });
    }
    if value.is_empty() || value.contains(" #") {
        return malformed_lock(
            path,
            format!("line {line_number}: invalid empty or annotated scalar"),
        );
    }
    Ok(value.to_owned())
}

fn parse_pnpm_package_key(
    entry: &str,
    path: &Path,
    line_number: usize,
) -> SupplyChainResult<PackageKey> {
    let entry = entry.strip_prefix('/').unwrap_or(entry);
    let base = entry.split_once('(').map_or(entry, |(base, _)| base);
    let split = base.rfind('@').filter(|index| *index > 0).ok_or_else(|| {
        SupplyChainError::MalformedLock {
            path: path.to_path_buf(),
            detail: format!("line {line_number}: package key `{entry}` has no exact name@version"),
        }
    })?;
    let (name, version_with_at) = base.split_at(split);
    let version = &version_with_at[1..];
    if name.is_empty() || version.is_empty() {
        return malformed_lock(
            path,
            format!("line {line_number}: package key `{entry}` has an empty name or version"),
        );
    }
    Ok(PackageKey {
        ecosystem: "npm".to_owned(),
        name: name.to_owned(),
        version: version.to_owned(),
    })
}

fn canonical_npm_tarball_url(name: &str, version: &str) -> String {
    let filename = name.rsplit('/').next().unwrap_or(name);
    format!("https://registry.npmjs.org/{name}/-/{filename}-{version}.tgz")
}

fn insert_package(
    packages: &mut BTreeMap<PackageKey, InventoryPackage>,
    package: InventoryPackage,
) -> SupplyChainResult<()> {
    let key = PackageKey {
        ecosystem: package.ecosystem.clone(),
        name: package.name.clone(),
        version: package.version.clone(),
    };
    if let Some(previous) = packages.insert(key.clone(), package) {
        return Err(SupplyChainError::MalformedLock {
            path: PathBuf::from(if key.ecosystem == "cargo" {
                CARGO_LOCK
            } else {
                PNPM_LOCK
            }),
            detail: format!(
                "duplicate package identity {}:{}@{} (previous source: {})",
                key.ecosystem, key.name, key.version, previous.source
            ),
        });
    }
    Ok(())
}

fn render_inventory(packages: &[InventoryPackage]) -> SupplyChainResult<String> {
    let inventory = LicenseInventory {
        schema_version: 2,
        generated_by: "cargo xtask licenses generate",
        authoritative_inputs: [CARGO_LOCK, PNPM_LOCK, REVIEWED_INPUTS],
        packages,
    };
    render_json(&inventory, "license inventory")
}

fn render_notice(packages: &[InventoryPackage]) -> SupplyChainResult<String> {
    let dependencies: Vec<_> = packages
        .iter()
        .filter(|package| package.kind == "dependency")
        .collect();
    let unresolved = dependencies
        .iter()
        .filter(|package| package.license_concluded == NOASSERTION)
        .count();
    let mut output = String::new();
    writeln!(
        output,
        "<!-- @generated by `cargo xtask licenses generate`; do not edit by hand. -->"
    )
    .map_err(notice_render_error)?;
    writeln!(output, "<!-- SPDX-License-Identifier: Apache-2.0 -->\n")
        .map_err(notice_render_error)?;
    writeln!(output, "# Third-party notices\n").map_err(notice_render_error)?;
    writeln!(
        output,
        "This deterministic Phase-00 inventory is derived from `{CARGO_LOCK}`, `{PNPM_LOCK}`, and `{REVIEWED_INPUTS}`. It inventories the locked development workspace; it is not a Phase-11 assembled-artifact notice set.\n"
    )
    .map_err(notice_render_error)?;
    writeln!(output, "## Locked dependency inventory\n").map_err(notice_render_error)?;
    writeln!(
        output,
        "| Ecosystem | Package | Version | Concluded license | Locked source |"
    )
    .map_err(notice_render_error)?;
    writeln!(output, "|---|---|---:|---|---|").map_err(notice_render_error)?;
    for package in dependencies {
        writeln!(
            output,
            "| {} | `{}` | `{}` | `{}` | `{}` |",
            markdown_cell(&package.ecosystem),
            markdown_cell(&package.name),
            markdown_cell(&package.version),
            markdown_cell(&package.license_concluded),
            markdown_cell(&package.source)
        )
        .map_err(notice_render_error)?;
    }
    writeln!(output).map_err(notice_render_error)?;
    writeln!(output, "## Review state\n").map_err(notice_render_error)?;
    writeln!(
        output,
        "{unresolved} locked dependency package(s) have `NOASSERTION` because no exact conclusion is present in the reviewed static input. Generation records those unresolved facts rather than guessing. A Phase-11 release remains blocked until every shipped component has a reviewed conclusion, required attribution, and corresponding license text."
    )
    .map_err(notice_render_error)?;
    Ok(output)
}

fn notice_render_error(error: fmt::Error) -> SupplyChainError {
    SupplyChainError::Render {
        artifact: NOTICE,
        detail: error.to_string(),
    }
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\n', '\r'], " ")
}

fn render_sbom(inventory: &Inventory) -> SupplyChainResult<String> {
    let mut spdx_packages = Vec::with_capacity(inventory.packages.len());
    let mut relationships = Vec::with_capacity(inventory.packages.len());
    for (index, package) in inventory.packages.iter().enumerate() {
        let spdx_id = format!(
            "SPDXRef-Package-{}-{}-{}-{:04}",
            spdx_slug(&package.ecosystem),
            spdx_slug(&package.name),
            spdx_slug(&package.version),
            index + 1
        );
        relationships.push(SpdxRelationship {
            spdx_element_id: "SPDXRef-DOCUMENT",
            relationship_type: "DESCRIBES",
            related_spdx_element: spdx_id.clone(),
        });
        let checksums = package.checksum.as_ref().map_or_else(Vec::new, |checksum| {
            vec![SpdxChecksum {
                algorithm: match checksum.algorithm {
                    ChecksumAlgorithm::Sha256 => "SHA256",
                    ChecksumAlgorithm::Sha512 => "SHA512",
                },
                checksum_value: checksum.value.clone(),
            }]
        });
        spdx_packages.push(SpdxPackage {
            spdx_id,
            name: package.name.clone(),
            version_info: package.version.clone(),
            download_location: if package.ecosystem == "npm" && package.kind == "dependency" {
                package.source.clone()
            } else {
                NOASSERTION.to_owned()
            },
            files_analyzed: false,
            license_concluded: package.license_concluded.clone(),
            license_declared: package.license_concluded.clone(),
            copyright_text: NOASSERTION,
            external_refs: [SpdxExternalRef {
                category: "PACKAGE-MANAGER",
                kind: "purl",
                locator: package_url(package),
            }],
            checksums,
        });
    }

    render_json(
        &SpdxDocument {
            spdx_version: "SPDX-2.3",
            data_license: "CC0-1.0",
            spdx_id: "SPDXRef-DOCUMENT",
            name: &inventory.reviewed.document_name,
            document_namespace: &inventory.reviewed.document_namespace,
            creation_info: SpdxCreationInfo {
                created: &inventory.reviewed.created,
                creators: ["Tool: eutheto xtask"],
            },
            packages: spdx_packages,
            relationships,
        },
        "SPDX-JSON SBOM",
    )
}

fn render_json(value: &impl Serialize, artifact: &'static str) -> SupplyChainResult<String> {
    let mut output =
        serde_json::to_string_pretty(value).map_err(|error| SupplyChainError::Render {
            artifact,
            detail: error.to_string(),
        })?;
    output.push('\n');
    Ok(output)
}

fn package_url(package: &InventoryPackage) -> String {
    format!(
        "pkg:{}/{}@{}",
        package.ecosystem,
        percent_encode(&package.name, package.ecosystem == "npm"),
        percent_encode(&package.version, false)
    )
}

fn percent_encode(value: &str, preserve_slash: bool) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || (preserve_slash && byte == b'/')
        {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn spdx_slug(value: &str) -> String {
    let slug: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect();
    if slug.is_empty() {
        "unknown".to_owned()
    } else {
        slug
    }
}

fn write_if_changed(
    repo_root: &Path,
    relative_path: &'static str,
    contents: &str,
) -> SupplyChainResult<()> {
    let path = repo_root.join(relative_path);
    if fs::read_to_string(&path).ok().as_deref() == Some(contents) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| SupplyChainError::WriteArtifact {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::write(&path, contents).map_err(|source| SupplyChainError::WriteArtifact { path, source })
}

fn check_outputs(repo_root: &Path, expected: &[(&'static str, String)]) -> SupplyChainResult<()> {
    let drifted: Vec<_> = expected
        .iter()
        .filter_map(|(relative_path, expected_contents)| {
            let path = repo_root.join(relative_path);
            match fs::read_to_string(path) {
                Ok(actual) if actual == *expected_contents => None,
                Ok(_) | Err(_) => Some(*relative_path),
            }
        })
        .collect();
    if drifted.is_empty() {
        Ok(())
    } else {
        Err(SupplyChainError::Drift { paths: drifted })
    }
}

fn read_required(path: &Path) -> SupplyChainResult<String> {
    fs::read_to_string(path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            SupplyChainError::MissingInput {
                path: path.to_path_buf(),
            }
        } else {
            SupplyChainError::ReadInput {
                path: path.to_path_buf(),
                source,
            }
        }
    })
}

fn malformed_lock<T>(path: &Path, detail: String) -> SupplyChainResult<T> {
    Err(SupplyChainError::MalformedLock {
        path: path.to_path_buf(),
        detail,
    })
}

fn looks_like_utc_timestamp(value: &str) -> bool {
    value.len() == 20
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.as_bytes().get(10) == Some(&b'T')
        && value.as_bytes().get(13) == Some(&b':')
        && value.as_bytes().get(16) == Some(&b':')
        && value.ends_with('Z')
        && value.bytes().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit()
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use base64::Engine as _;
    use serde_json::json;

    use super::{
        ChecksumAlgorithm, Inventory, InventoryPackage, PackageChecksum, PackageKey,
        ReviewedInputs, SupplyChainError, SupplyChainResult, canonical_npm_tarball_url,
        parse_pnpm_package_key, parse_pnpm_packages, parse_yaml_section_keys, percent_encode,
        render_inventory, render_sbom,
    };

    fn pnpm_package_with_resolution(resolution: &str) -> SupplyChainResult<super::PnpmPackage> {
        let contents =
            format!("packages:\n  package@1.2.3:\n    resolution: {resolution}\nsnapshots:\n");
        let path = Path::new("pnpm-lock.yaml");
        let entries = parse_yaml_section_keys(&contents, "packages", path)?;
        let mut packages = parse_pnpm_packages(&contents, entries, path)?;
        packages
            .pop()
            .ok_or_else(|| SupplyChainError::MalformedLock {
                path: path.to_path_buf(),
                detail: "synthetic package was not parsed".to_owned(),
            })
    }

    #[test]
    fn parses_scoped_pnpm_key_with_peer_context() -> SupplyChainResult<()> {
        let key = parse_pnpm_package_key(
            "@vue/server-renderer@3.5.42(vue@3.5.42)",
            Path::new("pnpm-lock.yaml"),
            1,
        )?;
        assert_eq!(
            key,
            PackageKey {
                ecosystem: "npm".to_owned(),
                name: "@vue/server-renderer".to_owned(),
                version: "3.5.42".to_owned(),
            }
        );
        Ok(())
    }

    #[test]
    fn parses_nested_and_empty_flow_map_pnpm_section_keys() -> SupplyChainResult<()> {
        let contents = concat!(
            "importers:\n",
            "  .: {}\n",
            "  'workspace:tools':\n",
            "    dependencies: {}\n",
            "packages:\n",
            "  name@1.2.3: {}\n",
            "  nested@4.5.6:\n",
            "    resolution: {}\n",
        );
        let path = Path::new("pnpm-lock.yaml");

        assert_eq!(
            parse_yaml_section_keys(contents, "importers", path)?,
            BTreeMap::from([(".".to_owned(), 2), ("workspace:tools".to_owned(), 3)])
        );
        assert_eq!(
            parse_yaml_section_keys(contents, "packages", path)?,
            BTreeMap::from([("name@1.2.3".to_owned(), 6), ("nested@4.5.6".to_owned(), 7),])
        );
        Ok(())
    }

    #[test]
    fn rejects_inline_pnpm_section_values_and_comments() {
        for entry in [
            "name@1.2.3: false",
            "name@1.2.3: { resolution: registry }",
            "name@1.2.3: # empty package",
        ] {
            let contents = format!("packages:\n  {entry}\n");
            assert_eq!(
                parse_yaml_section_keys(&contents, "packages", Path::new("pnpm-lock.yaml"))
                    .map_err(|error| error.to_string()),
                Err(
                    "malformed lock file pnpm-lock.yaml: line 2: malformed key in `packages` section"
                        .to_owned()
                )
            );
        }
    }

    #[test]
    fn preserves_duplicate_pnpm_section_key_diagnostic_across_mapping_forms() {
        let contents = "packages:\n  name@1.2.3:\n  name@1.2.3: {}\n";
        assert_eq!(
            parse_yaml_section_keys(contents, "packages", Path::new("pnpm-lock.yaml"))
                .map_err(|error| error.to_string()),
            Err(
                "malformed lock file pnpm-lock.yaml: line 3: duplicate `packages` key `name@1.2.3`"
                    .to_owned()
            )
        );
    }

    #[test]
    fn purl_encoding_preserves_npm_namespace_separator() {
        assert_eq!(percent_encode("@tauri-apps/api", true), "%40tauri-apps/api");
    }

    #[test]
    fn canonical_scoped_npm_tarball_url_uses_unscoped_filename() {
        assert_eq!(
            canonical_npm_tarball_url("@tauri-apps/api", "2.11.1"),
            "https://registry.npmjs.org/@tauri-apps/api/-/api-2.11.1.tgz"
        );
    }

    #[test]
    fn decodes_sha512_integrity_to_canonical_hex() -> SupplyChainResult<()> {
        let encoded = base64::engine::general_purpose::STANDARD.encode([0_u8; 64]);
        let package = pnpm_package_with_resolution(&format!("{{integrity: sha512-{encoded}}}"))?;
        assert_eq!(
            package.checksum,
            PackageChecksum {
                algorithm: ChecksumAlgorithm::Sha512,
                value: "00".repeat(64),
            }
        );
        assert_eq!(
            package.source,
            "https://registry.npmjs.org/package/-/package-1.2.3.tgz"
        );
        Ok(())
    }

    #[test]
    fn preserves_explicit_pnpm_tarball_source() -> SupplyChainResult<()> {
        let encoded = base64::engine::general_purpose::STANDARD.encode([1_u8; 64]);
        let tarball = "https://registry.example.test/package-1.2.3.tgz";
        let package = pnpm_package_with_resolution(&format!(
            "{{integrity: sha512-{encoded}, tarball: {tarball}}}"
        ))?;
        assert_eq!(package.source, tarball);
        Ok(())
    }

    #[test]
    fn rejects_malformed_sha512_base64_at_resolution_line() {
        assert_eq!(
            pnpm_package_with_resolution("{integrity: sha512-***}")
                .map_err(|error| error.to_string()),
            Err(
                "malformed lock file pnpm-lock.yaml: line 3: malformed SHA-512 integrity: Invalid symbol 42, offset 0."
                    .to_owned()
            )
        );
    }

    #[test]
    fn rejects_wrong_sha512_integrity_length_at_resolution_line() {
        let encoded = base64::engine::general_purpose::STANDARD.encode([0_u8; 63]);
        assert_eq!(
            pnpm_package_with_resolution(&format!("{{integrity: sha512-{encoded}}}"))
                .map_err(|error| error.to_string()),
            Err(
                "malformed lock file pnpm-lock.yaml: line 3: SHA-512 integrity decoded to 63 bytes; expected 64"
                    .to_owned()
            )
        );
    }

    #[test]
    fn rejects_unsupported_integrity_algorithm_at_resolution_line() {
        let encoded = base64::engine::general_purpose::STANDARD.encode([0_u8; 64]);
        assert_eq!(
            pnpm_package_with_resolution(&format!("{{integrity: sha256-{encoded}}}"))
                .map_err(|error| error.to_string()),
            Err(
                "malformed lock file pnpm-lock.yaml: line 3: unsupported package integrity algorithm `sha256`"
                    .to_owned()
            )
        );
    }

    #[test]
    fn rejects_package_without_resolution_at_package_line() -> SupplyChainResult<()> {
        let contents = "packages:\n  package@1.2.3:\n    engines: {}\nsnapshots:\n";
        let path = Path::new("pnpm-lock.yaml");
        let entries = parse_yaml_section_keys(contents, "packages", path)?;
        assert_eq!(
            parse_pnpm_packages(contents, entries, path).map_err(|error| error.to_string()),
            Err(
                "malformed lock file pnpm-lock.yaml: line 2: package `package@1.2.3` is missing resolution"
                    .to_owned()
            )
        );
        Ok(())
    }

    #[test]
    fn renders_spdx_checksum_algorithms_and_npm_download_location() -> SupplyChainResult<()> {
        let npm_source = canonical_npm_tarball_url("@scope/package", "2.0.0");
        let inventory = Inventory {
            reviewed: ReviewedInputs {
                schema_version: 1,
                document_name: "test".to_owned(),
                document_namespace: "https://example.test/sbom".to_owned(),
                created: "2026-01-01T00:00:00Z".to_owned(),
                workspace_packages: Vec::new(),
                license_conclusions: Vec::new(),
            },
            packages: vec![
                InventoryPackage {
                    ecosystem: "cargo".to_owned(),
                    name: "crate".to_owned(),
                    version: "1.0.0".to_owned(),
                    kind: "dependency".to_owned(),
                    license_concluded: "MIT".to_owned(),
                    source: "registry+https://github.com/rust-lang/crates.io-index".to_owned(),
                    checksum: Some(PackageChecksum {
                        algorithm: ChecksumAlgorithm::Sha256,
                        value: "11".repeat(32),
                    }),
                },
                InventoryPackage {
                    ecosystem: "npm".to_owned(),
                    name: "@scope/package".to_owned(),
                    version: "2.0.0".to_owned(),
                    kind: "dependency".to_owned(),
                    license_concluded: "MIT".to_owned(),
                    source: npm_source.clone(),
                    checksum: Some(PackageChecksum {
                        algorithm: ChecksumAlgorithm::Sha512,
                        value: "22".repeat(64),
                    }),
                },
            ],
        };

        let rendered_inventory = render_inventory(&inventory.packages)?;
        let rendered_inventory: serde_json::Value = serde_json::from_str(&rendered_inventory)
            .map_err(|error| SupplyChainError::Render {
                artifact: "test inventory",
                detail: error.to_string(),
            })?;
        assert_eq!(rendered_inventory["schemaVersion"], 2);
        assert_eq!(
            rendered_inventory["packages"][0]["checksum"],
            json!({"algorithm": "SHA256", "value": "11".repeat(32)})
        );
        assert_eq!(
            rendered_inventory["packages"][1]["checksum"],
            json!({"algorithm": "SHA512", "value": "22".repeat(64)})
        );

        let rendered = render_sbom(&inventory)?;
        let rendered: serde_json::Value =
            serde_json::from_str(&rendered).map_err(|error| SupplyChainError::Render {
                artifact: "test SPDX",
                detail: error.to_string(),
            })?;
        assert_eq!(
            rendered["packages"][0]["checksums"],
            json!([{"algorithm": "SHA256", "checksumValue": "11".repeat(32)}])
        );
        assert_eq!(
            rendered["packages"][1]["checksums"],
            json!([{"algorithm": "SHA512", "checksumValue": "22".repeat(64)}])
        );
        assert_eq!(rendered["packages"][1]["downloadLocation"], npm_source);
        Ok(())
    }
}
