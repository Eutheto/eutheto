use std::env;
use std::fs::{self, File, OpenOptions};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

const NATIVE_BUILD_SCRIPT: &str = "workers/ortools/cmake/native_windows_build.cmake";
const NATIVE_BUILD_ROOT: &str = ".cache/ortools-native/windows-x86_64";
const NATIVE_WORK: &str = ".cache/ortools-native/windows-x86_64/work";
const NATIVE_STAGING: &str = ".cache/ortools-native/windows-x86_64/staging";
const NATIVE_COMPILER_VERSION: &str =
    ".cache/ortools-native/windows-x86_64/work/compiler-version.txt";
const SOLVER_SOURCE_DATE: &str = "2026-09-02T00:00:00Z";
const NATIVE_CURRENT: &str = ".cache/ortools-native/windows-x86_64/current";
const NATIVE_WORKER: &str = ".cache/ortools-native/windows-x86_64/current/bin/ortools-worker.exe";

#[allow(clippy::too_many_arguments)]
pub(crate) fn finalize_artifact(
    authority_root: &Path,
    work_root: &Path,
    artifact_root: &Path,
    target_triple: &str,
    compiler_identity: &str,
    compiler_version: &str,
    source_date: &str,
) -> Result<()> {
    crate::solver_artifact::finalize(crate::solver_artifact::FinalizeOptions {
        authority_root,
        work_root,
        artifact_root,
        target_triple,
        compiler_identity,
        compiler_version,
        source_date,
    })?;
    Ok(())
}

pub(crate) fn assemble_manifest(
    source_contract: &Path,
    protocol_schema: &Path,
    protocol_policy: &Path,
    build_evidence: &Path,
    payload_evidence: &Path,
    artifact_root: &Path,
) -> Result<()> {
    crate::solver_manifest::assemble(crate::solver_manifest::AssembleOptions {
        source_contract,
        protocol_schema,
        protocol_policy,
        build_evidence,
        payload_evidence,
        artifact_root,
    })?;
    Ok(())
}

pub(crate) fn validate_manifest(
    source_contract: &Path,
    protocol_schema: &Path,
    protocol_policy: &Path,
    artifact_root: &Path,
) -> Result<()> {
    crate::solver_manifest::validate(crate::solver_manifest::ValidateOptions {
        source_contract,
        protocol_schema,
        protocol_policy,
        artifact_root,
    })?;
    Ok(())
}

pub(crate) fn build_native(repo_root: &Path) -> Result<()> {
    require_native_target(env::consts::OS, env::consts::ARCH)?;
    ensure!(
        repo_root.is_absolute(),
        "native worker repository root must be absolute: {}",
        repo_root.display()
    );
    // CARGO_MANIFEST_DIR is already absolute. Avoid canonicalize on Windows:
    // it emits a verbatim `\\?\` path that external Git does not accept.
    let repository = repo_root.to_path_buf();
    let native_root = repository.join(NATIVE_BUILD_ROOT);
    let mut build = NativeBuildGuard::acquire(&repository, &native_root)?;
    build.prepare()?;

    verify_generated_build_input(
        &repository,
        crate::source_contract::generated_file(&repository)?,
    )?;
    verify_generated_build_input(
        &repository,
        crate::source_contract::dependency_sources_generated_file()?,
    )?;
    let script = repository.join(NATIVE_BUILD_SCRIPT);
    ensure!(
        script.is_file(),
        "native Windows worker build script is missing: {}",
        script.display()
    );
    let repository_arg = cmake_path_argument("REPOSITORY_ROOT", &repository)?;
    let native_root_arg = cmake_path_argument("NATIVE_ROOT", &native_root)?;
    let status = Command::new("cmake")
        .arg(repository_arg)
        .arg(native_root_arg)
        .arg("-P")
        .arg(&script)
        .status()
        .context(
            "failed to start CMake; run this command from an x64 Visual Studio developer environment with CMake and Ninja available",
        )?;
    ensure!(
        status.success(),
        "native Windows OR-Tools worker build failed with {status}"
    );

    let compiler_version = read_compiler_version(&repository.join(NATIVE_COMPILER_VERSION))?;
    let staging = repository.join(NATIVE_STAGING);
    crate::solver_artifact::finalize(crate::solver_artifact::FinalizeOptions {
        authority_root: &repository,
        work_root: &repository.join(NATIVE_WORK),
        artifact_root: &staging,
        target_triple: "x86_64-pc-windows-msvc",
        compiler_identity: "msvc",
        compiler_version: &compiler_version,
        source_date: SOLVER_SOURCE_DATE,
    })?;
    verify_native_artifact(&repository, &staging)?;
    fs::remove_dir_all(repository.join(NATIVE_WORK))
        .context("failed to remove private native worker build inputs")?;
    let current = repository.join(NATIVE_CURRENT);
    publish_staging(&staging, &current)?;
    build.commit()?;
    println!(
        "built native Windows worker: {}",
        repository.join(NATIVE_WORKER).display()
    );
    Ok(())
}
#[derive(Debug)]
struct NativeBuildGuard {
    root: PathBuf,
    lock: Option<File>,
    committed: bool,
}

impl NativeBuildGuard {
    fn acquire(repository: &Path, root: &Path) -> Result<Self> {
        create_private_directory_chain(repository, root)?;
        let lock_path = root.join("build.lock");
        let lock = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .with_context(|| {
                format!(
                    "native Windows worker build is already locked at {}",
                    lock_path.display()
                )
            })?;
        Ok(Self {
            root: root.to_path_buf(),
            lock: Some(lock),
            committed: false,
        })
    }

    fn prepare(&self) -> Result<()> {
        let current = self.root.join("current");
        let previous = self.root.join("previous");
        let current_exists = path_exists_no_follow(&current)?;
        let previous_exists = path_exists_no_follow(&previous)?;
        match (current_exists, previous_exists) {
            (false, true) => fs::rename(&previous, &current)
                .context("failed to recover the previous native worker publication")?,
            (true, true) => remove_path_no_follow(&previous)
                .context("failed to remove obsolete native worker publication backup")?,
            _ => {}
        }
        for name in ["work", "staging"] {
            remove_path_no_follow(&self.root.join(name))?;
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        drop(self.lock.take());
        let lock_path = self.root.join("build.lock");
        fs::remove_file(&lock_path).with_context(|| {
            format!(
                "failed to release native build lock {}",
                lock_path.display()
            )
        })?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for NativeBuildGuard {
    fn drop(&mut self) {
        if !self.committed {
            for name in ["work", "staging"] {
                let _ = remove_path_no_follow(&self.root.join(name));
            }
        }
        drop(self.lock.take());
        let _ = fs::remove_file(self.root.join("build.lock"));
    }
}

fn create_private_directory_chain(repository: &Path, root: &Path) -> Result<()> {
    let repository_metadata = fs::symlink_metadata(repository)
        .with_context(|| format!("failed to inspect repository root {}", repository.display()))?;
    ensure!(
        repository_metadata.is_dir() && !is_link_like(&repository_metadata),
        "repository root must be a regular directory: {}",
        repository.display()
    );
    let relative = root
        .strip_prefix(repository)
        .context("native worker state root must be inside the repository")?;
    let mut directory = repository.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            bail!("native worker state root contains an unsafe component");
        };
        directory.push(component);
        let metadata = match fs::symlink_metadata(&directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&directory).with_context(|| {
                    format!(
                        "failed to create native worker directory {}",
                        directory.display()
                    )
                })?;
                fs::symlink_metadata(&directory)?
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect native worker directory {}",
                        directory.display()
                    )
                });
            }
        };
        ensure!(
            metadata.is_dir() && !is_link_like(&metadata),
            "native worker path component must be a regular directory: {}",
            directory.display()
        );
    }
    Ok(())
}

fn path_exists_no_follow(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", path.display())),
    }
}

fn is_link_like(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}
fn remove_path_no_follow(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to inspect native build path {}", path.display())
            });
        }
    };
    if metadata.is_dir() && !is_link_like(&metadata) {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove native build directory {}", path.display()))
    } else {
        fs::remove_file(path)
            .with_context(|| format!("failed to remove native build entry {}", path.display()))
    }
}

fn read_compiler_version(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path).with_context(|| {
        format!(
            "failed to inspect compiler version authority {}",
            path.display()
        )
    })?;
    ensure!(
        metadata.is_file() && !is_link_like(&metadata) && metadata.len() <= 128,
        "compiler version authority must be a bounded regular file: {}",
        path.display()
    );
    let bytes = fs::read(path).with_context(|| {
        format!(
            "failed to read compiler version authority {}",
            path.display()
        )
    })?;
    let value = std::str::from_utf8(&bytes)
        .context("compiler version authority is not UTF-8")?
        .strip_suffix('\n')
        .context("compiler version authority must end with one newline")?;
    ensure!(
        !value.is_empty()
            && !value.contains('\n')
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.+-".contains(&byte)),
        "compiler version authority is not normalized"
    );
    Ok(value.to_owned())
}

fn publish_staging(staging: &Path, current: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(staging).with_context(|| {
        format!(
            "native worker staging artifact is missing: {}",
            staging.display()
        )
    })?;
    ensure!(
        metadata.is_dir() && !is_link_like(&metadata),
        "native worker staging artifact must be a regular directory: {}",
        staging.display()
    );
    let parent = current
        .parent()
        .context("native worker publication has no parent")?;
    let previous = parent.join("previous");
    ensure!(
        !path_exists_no_follow(&previous)?,
        "native worker publication backup already exists: {}",
        previous.display()
    );
    let had_current = path_exists_no_follow(current)?;
    if had_current {
        let current_metadata = fs::symlink_metadata(current)?;
        ensure!(
            current_metadata.is_dir() && !is_link_like(&current_metadata),
            "native worker current artifact must be a regular directory: {}",
            current.display()
        );
        fs::rename(current, &previous)
            .context("failed to move the previous native worker publication aside")?;
    }
    if let Err(error) = fs::rename(staging, current) {
        if had_current {
            fs::rename(&previous, current)
                .context("failed to restore the previous native worker publication")?;
        }
        return Err(error).with_context(|| {
            format!(
                "failed to atomically publish native worker {} as {}",
                staging.display(),
                current.display()
            )
        });
    }
    if had_current {
        remove_path_no_follow(&previous)
            .context("failed to remove the previous native worker publication")?;
    }
    Ok(())
}

fn verify_native_artifact(repository: &Path, artifact_root: &Path) -> Result<()> {
    let worker = artifact_root.join("bin/ortools-worker.exe");
    let metadata = fs::symlink_metadata(&worker)
        .with_context(|| format!("published native worker is missing: {}", worker.display()))?;
    ensure!(
        metadata.is_file() && !is_link_like(&metadata),
        "published native worker must be a regular file: {}",
        worker.display()
    );
    crate::solver_manifest::validate(crate::solver_manifest::ValidateOptions {
        source_contract: &repository.join("workers/ortools/source-contract.json"),
        protocol_schema: &repository.join("protocol/solver-worker.proto"),
        protocol_policy: &repository.join("protocol/version.json"),
        artifact_root,
    })?;
    Ok(())
}

fn verify_generated_build_input(
    repository: &Path,
    (relative_path, expected): (String, Vec<u8>),
) -> Result<()> {
    let path = repository.join(&relative_path);
    let actual = fs::read(&path)
        .with_context(|| format!("failed to read generated build input {}", path.display()))?;
    ensure!(
        actual == expected,
        "{relative_path} is stale or modified; run `cargo xtask generate`"
    );
    Ok(())
}

fn cmake_path_argument(name: &str, path: &Path) -> Result<String> {
    let path = path
        .to_str()
        .with_context(|| format!("{name} is not valid Unicode: {}", path.display()))?;
    Ok(format!("-D{name}={path}"))
}

fn require_native_target(os: &str, arch: &str) -> Result<()> {
    ensure!(
        os == "windows" && arch == "x86_64",
        "solver build-native requires native Windows x86_64; current target is {os}/{arch}"
    );
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::{
        NativeBuildGuard, publish_staging, read_compiler_version, require_native_target,
        verify_generated_build_input,
    };
    #[test]
    fn accepts_only_native_windows_x86_64() {
        assert!(require_native_target("windows", "x86_64").is_ok());

        let windows_arm = require_native_target("windows", "aarch64").unwrap_err();
        assert_eq!(
            windows_arm.to_string(),
            "solver build-native requires native Windows x86_64; current target is windows/aarch64"
        );

        let linux = require_native_target("linux", "x86_64").unwrap_err();
        assert_eq!(
            linux.to_string(),
            "solver build-native requires native Windows x86_64; current target is linux/x86_64"
        );
    }

    #[test]
    fn rejects_modified_generated_build_input() {
        let repository = TempDir::new().unwrap();
        let relative_path = "workers/ortools/dependency-sources.json";
        let path = repository.path().join(relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"modified\n").unwrap();

        let error = verify_generated_build_input(
            repository.path(),
            (relative_path.to_owned(), b"generated\n".to_vec()),
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "workers/ortools/dependency-sources.json is stale or modified; run `cargo xtask generate`"
        );
    }

    #[test]
    fn native_build_lock_is_exclusive_and_released() {
        let repository = TempDir::new().unwrap();
        let root = repository.path().join("state");
        let first = NativeBuildGuard::acquire(repository.path(), &root).unwrap();
        let error = NativeBuildGuard::acquire(repository.path(), &root).unwrap_err();
        assert!(error.to_string().contains("already locked"));
        drop(first);
        let _next = NativeBuildGuard::acquire(repository.path(), &root).unwrap();
    }

    #[test]
    fn failed_native_build_cleans_private_paths_and_preserves_current() {
        let repository = TempDir::new().unwrap();
        let root = repository.path().join("state");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("current")).unwrap();
        fs::write(root.join("current/payload"), b"payload").unwrap();
        {
            let guard = NativeBuildGuard::acquire(repository.path(), &root).unwrap();
            guard.prepare().unwrap();
            for name in ["work", "staging"] {
                let path = root.join(name);
                fs::create_dir(&path).unwrap();
                fs::write(path.join("payload"), b"private").unwrap();
            }
            drop(guard);
        }
        for name in ["work", "staging", "build.lock"] {
            assert!(!root.join(name).exists());
        }
        assert_eq!(fs::read(root.join("current/payload")).unwrap(), b"payload");
    }

    #[test]
    fn replaces_current_without_exposing_partial_contents() {
        let root = TempDir::new().unwrap();
        let staging = root.path().join("staging");
        let current = root.path().join("current");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("payload"), b"new").unwrap();
        fs::create_dir(&current).unwrap();
        fs::write(current.join("payload"), b"old").unwrap();

        publish_staging(&staging, &current).unwrap();

        assert!(!staging.exists());
        assert!(!root.path().join("previous").exists());
        assert_eq!(fs::read(current.join("payload")).unwrap(), b"new");
    }

    #[test]
    fn interrupted_publication_recovers_previous_artifact() {
        let repository = TempDir::new().unwrap();
        let root = repository.path().join("state");
        let guard = NativeBuildGuard::acquire(repository.path(), &root).unwrap();
        let previous = root.join("previous");
        fs::create_dir(&previous).unwrap();
        fs::write(previous.join("payload"), b"old").unwrap();

        guard.prepare().unwrap();

        assert!(!previous.exists());
        assert_eq!(fs::read(root.join("current/payload")).unwrap(), b"old");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_linked_native_state_ancestor() {
        use std::os::unix::fs::symlink;

        let repository = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let linked = repository.path().join("linked");
        symlink(outside.path(), &linked).unwrap();

        assert!(NativeBuildGuard::acquire(repository.path(), &linked.join("state")).is_err());
    }

    #[test]
    fn compiler_version_authority_is_bounded_and_normalized() {
        let root = TempDir::new().unwrap();
        let version = root.path().join("compiler-version.txt");
        fs::write(&version, b"19.44.35217.0\n").unwrap();
        assert_eq!(read_compiler_version(&version).unwrap(), "19.44.35217.0");

        fs::write(&version, b"19.44\nextra\n").unwrap();
        assert!(read_compiler_version(&version).is_err());
    }
}
