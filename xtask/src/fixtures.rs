use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

pub fn validate(repo_root: &Path) -> Result<()> {
    let roots = discover_fixture_roots(repo_root)?;
    let mut file_count = 0_u64;

    for root in &roots {
        file_count = file_count
            .checked_add(validate_tree(root)?)
            .context("fixture file count overflowed")?;
    }

    if file_count == 0 {
        bail!("fixture discovery found no files in the configured fixture roots");
    }

    println!(
        "validated {file_count} fixture file(s) across {} root(s)",
        roots.len()
    );
    Ok(())
}

fn discover_fixture_roots(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let mut roots = vec![required_directory(repo_root, "protocol/golden")?];

    if let Some(domains) = optional_directory(repo_root, "domains")? {
        for child in child_directories(&domains)? {
            roots.push(required_directory(&child, "fixtures")?);
        }
    }

    if let Some(corpus) = optional_directory(repo_root, "benchmarks/corpus")? {
        roots.extend(child_directories(&corpus)?);
    }

    if let Some(examples) = optional_directory(repo_root, "examples")? {
        roots.extend(child_directories(&examples)?);
    }

    roots.sort();
    roots.dedup();
    Ok(roots)
}

fn required_directory(base: &Path, relative: impl AsRef<Path>) -> Result<PathBuf> {
    let path = base.join(relative);
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("required fixture directory is missing: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "fixture root must be a real directory, not a file or symlink: {}",
            path.display()
        );
    }
    Ok(path)
}

fn optional_directory(base: &Path, relative: impl AsRef<Path>) -> Result<Option<PathBuf>> {
    let path = base.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect fixture root {}", path.display()));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "fixture root must be a real directory, not a file or symlink: {}",
            path.display()
        );
    }
    Ok(Some(path))
}

fn child_directories(base: &Path) -> Result<Vec<PathBuf>> {
    let mut children = Vec::new();
    for entry in fs::read_dir(base)
        .with_context(|| format!("failed to read fixture parent {}", base.display()))?
    {
        let entry = entry.with_context(|| format!("failed to inspect {}", base.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", entry.path().display()))?;
        if file_type.is_symlink() {
            bail!(
                "fixture discovery does not follow symlinks: {}",
                entry.path().display()
            );
        }
        if file_type.is_dir() {
            children.push(entry.path());
        }
    }
    children.sort();
    Ok(children)
}

fn validate_tree(root: &Path) -> Result<u64> {
    let mut pending = vec![root.to_path_buf()];
    let mut count = 0_u64;

    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(&directory)
            .with_context(|| format!("failed to read fixture directory {}", directory.display()))?
            .collect::<std::io::Result<Vec<_>>>()
            .with_context(|| format!("failed to inspect fixtures in {}", directory.display()))?;
        entries.sort_by_key(fs::DirEntry::file_name);

        for entry in entries {
            let path = entry.path();
            let name = entry.file_name();
            if name.to_str().is_none() {
                bail!("fixture path is not valid UTF-8: {}", path.display());
            }
            if name.to_string_lossy().starts_with('.') {
                continue;
            }

            let file_type = entry
                .file_type()
                .with_context(|| format!("failed to inspect fixture {}", path.display()))?;
            if file_type.is_symlink() {
                bail!("fixture trees may not contain symlinks: {}", path.display());
            }
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() {
                let metadata = entry
                    .metadata()
                    .with_context(|| format!("failed to inspect fixture {}", path.display()))?;
                if metadata.len() == 0 {
                    bail!("fixture file is empty: {}", path.display());
                }
                count = count
                    .checked_add(1)
                    .context("fixture file count overflowed")?;
            } else {
                bail!("unsupported fixture filesystem entry: {}", path.display());
            }
        }
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::discover_fixture_roots;
    use std::path::Path;

    #[test]
    fn repository_fixture_roots_are_discoverable() -> anyhow::Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| anyhow::anyhow!("xtask manifest has no repository parent"))?;
        let roots = discover_fixture_roots(root)?;

        assert!(roots.iter().any(|path| path.ends_with("protocol/golden")));
        Ok(())
    }
}
