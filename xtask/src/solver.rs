use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
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

const SIDECAR_ROOT: &str = "apps/desktop/src-tauri/sidecar";
const SIDECAR_STAGING: &str = "sidecar.staging";
const SIDECAR_PREVIOUS: &str = "sidecar.previous";
const SIDECAR_LOCK: &str = "sidecar.lock";
const SIDECAR_BUILD_INSTRUCTION: &str = "cargo xtask solver build-desktop";
const SOLVER_MANIFEST_DIGEST_ENV: &str = "EUTHETO_ORTOOLS_MANIFEST_SHA256";
const MAX_SIDECAR_TREE_ENTRIES: usize = 256;
const MAX_SIDECAR_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_SIDECAR_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;

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

pub(crate) fn install_from_nix(repo_root: &Path) -> Result<()> {
    ensure_repository_root(repo_root)?;
    #[cfg(windows)]
    {
        bail!("Nix worker installation is unavailable on native Windows; use build-native")
    }
    #[cfg(not(windows))]
    {
        let artifact_root = build_pinned_nix_artifact(repo_root)?;
        verify_current_nix_result(repo_root, &artifact_root)?;
        let (_manifest_sha256, mut sidecar_lease) =
            stage_solver_artifact(repo_root, &artifact_root)?;
        sidecar_lease.commit();
        println!("{SIDECAR_BUILD_INSTRUCTION}");
        Ok(())
    }
}

pub(crate) fn build_desktop(repo_root: &Path) -> Result<()> {
    ensure_repository_root(repo_root)?;
    #[cfg(windows)]
    let (artifact_root, mut native_lease) = build_native_artifact(repo_root)?;
    #[cfg(not(windows))]
    let artifact_root = build_pinned_nix_artifact(repo_root)?;
    let (manifest_sha256, mut sidecar_lease) = stage_solver_artifact(repo_root, &artifact_root)?;
    let mut command = Command::new("pnpm");
    command
        .args([
            "--filter",
            "@eutheto/desktop",
            "run",
            "tauri",
            "build",
            "--features",
            "bundled-ortools",
            "--config",
            "src-tauri/tauri.sidecar.conf.json",
        ])
        .current_dir(repo_root)
        .env(SOLVER_MANIFEST_DIGEST_ENV, manifest_sha256);
    #[cfg(target_os = "linux")]
    command.args(["--bundles", "deb,rpm"]);
    let build_result = command
        .status()
        .context("failed to start the bundle-aware Tauri desktop build");
    sidecar_lease.commit();
    #[cfg(windows)]
    native_lease.commit()?;
    let status = build_result?;
    ensure!(
        status.success(),
        "bundle-aware Tauri desktop build failed with {status}"
    );
    Ok(())
}

fn ensure_repository_root(repo_root: &Path) -> Result<()> {
    ensure!(
        repo_root.is_absolute(),
        "solver repository root must be absolute: {}",
        repo_root.display()
    );
    Ok(())
}

#[cfg(not(windows))]
fn build_pinned_nix_artifact(repo_root: &Path) -> Result<PathBuf> {
    let output = Command::new("nix")
        .args([
            "build",
            ".#ortools-worker",
            "--no-link",
            "--print-out-paths",
        ])
        .current_dir(repo_root)
        .output()
        .context("failed to start the pinned Nix solver build")?;
    ensure!(
        output.status.success(),
        "pinned Nix solver build failed with {}",
        output.status
    );
    ensure!(
        output.stdout.len() <= 4096,
        "pinned Nix solver build returned excessive path output"
    );
    let stdout =
        std::str::from_utf8(&output.stdout).context("pinned Nix solver path is not UTF-8")?;
    let paths = stdout.lines().collect::<Vec<_>>();
    ensure!(
        paths.len() == 1 && !paths[0].is_empty(),
        "pinned Nix solver build must return exactly one output path"
    );
    let artifact_root = PathBuf::from(paths[0]);
    ensure!(
        artifact_root.is_absolute() && artifact_root.starts_with("/nix/store"),
        "pinned Nix solver output must be an absolute Nix store path"
    );
    let resolved = fs::canonicalize(&artifact_root)
        .context("failed to resolve pinned Nix solver output path")?;
    ensure!(
        resolved == artifact_root,
        "pinned Nix solver output must be a direct Nix store directory"
    );
    let metadata = fs::symlink_metadata(&artifact_root)
        .context("failed to inspect pinned Nix solver output path")?;
    ensure!(
        metadata.is_dir() && !is_link_like(&metadata),
        "pinned Nix solver output must be a regular directory"
    );
    Ok(artifact_root)
}

#[cfg(not(windows))]
fn verify_current_nix_result(repo_root: &Path, expected_artifact: &Path) -> Result<()> {
    let result = repo_root.join("result");
    let metadata = fs::symlink_metadata(&result).with_context(|| {
        format!(
            "current Nix result is missing at {}; run `just worker-build-nix`",
            result.display()
        )
    })?;
    ensure!(
        metadata.file_type().is_symlink(),
        "current Nix result must be the Nix-created `result` symlink"
    );
    let resolved = fs::canonicalize(&result).context("failed to resolve current Nix result")?;
    ensure!(
        resolved == expected_artifact,
        "current Nix result does not match the pinned .#ortools-worker output; run `just worker-build-nix`"
    );
    Ok(())
}

pub(crate) fn build_native(repo_root: &Path) -> Result<()> {
    let (artifact_root, mut build_lease) = build_native_artifact(repo_root)?;
    let (_manifest_sha256, mut sidecar_lease) = stage_solver_artifact(repo_root, &artifact_root)?;
    sidecar_lease.commit();
    build_lease.commit()?;
    Ok(())
}

fn build_native_artifact(repo_root: &Path) -> Result<(PathBuf, NativeBuildGuard)> {
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
    let build = NativeBuildGuard::acquire(&repository, &native_root)?;
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
    println!(
        "built native Windows worker: {}",
        repository.join(NATIVE_WORKER).display()
    );
    Ok((current, build))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SidecarTarget {
    artifact_worker: &'static str,
    bundled_worker: &'static str,
}

fn sidecar_target(target_triple: &str) -> Result<SidecarTarget> {
    match target_triple {
        "x86_64-pc-windows-msvc" => Ok(SidecarTarget {
            artifact_worker: "bin/ortools-worker.exe",
            bundled_worker: "ortools-worker-x86_64-pc-windows-msvc.exe",
        }),
        "x86_64-apple-darwin" => Ok(SidecarTarget {
            artifact_worker: "bin/ortools-worker",
            bundled_worker: "ortools-worker-x86_64-apple-darwin",
        }),
        "aarch64-apple-darwin" => Ok(SidecarTarget {
            artifact_worker: "bin/ortools-worker",
            bundled_worker: "ortools-worker-aarch64-apple-darwin",
        }),
        "x86_64-unknown-linux-gnu" => Ok(SidecarTarget {
            artifact_worker: "bin/ortools-worker",
            bundled_worker: "ortools-worker-x86_64-unknown-linux-gnu",
        }),
        _ => bail!("unsupported solver sidecar target: {target_triple}"),
    }
}

fn stage_solver_artifact(
    repository: &Path,
    artifact_root: &Path,
) -> Result<(String, SidecarStageGuard)> {
    let source_manifest_sha256 =
        crate::solver_manifest::validate(crate::solver_manifest::ValidateOptions {
            source_contract: &repository.join("workers/ortools/source-contract.json"),
            protocol_schema: &repository.join("protocol/solver-worker.proto"),
            protocol_policy: &repository.join("protocol/version.json"),
            artifact_root,
        })?;
    let target = sidecar_target(&read_manifest_target(artifact_root)?)?;
    let current = repository.join(SIDECAR_ROOT);
    let parent = current
        .parent()
        .context("solver sidecar destination has no parent")?;
    let stage = SidecarStageGuard::acquire(repository, parent)?;
    stage.prepare()?;
    copy_artifact_tree(artifact_root, &stage.staging(), target)?;
    let staged_manifest_sha256 = validate_staged_artifact(repository, &stage.staging(), target)?;
    ensure!(
        staged_manifest_sha256 == source_manifest_sha256,
        "staged solver manifest digest differs from the pinned artifact"
    );
    publish_directory(&stage.staging(), &current, &stage.previous())?;
    Ok((source_manifest_sha256, stage))
}

fn read_manifest_target(artifact_root: &Path) -> Result<String> {
    const MAX_MANIFEST_BYTES: u64 = 65_536;

    let path = artifact_root.join("solver-manifest.json");
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect solver manifest {}", path.display()))?;
    ensure!(
        metadata.is_file() && !is_link_like(&metadata) && metadata.len() <= MAX_MANIFEST_BYTES,
        "solver manifest must be a bounded regular file: {}",
        path.display()
    );
    let mut file = open_regular_file_no_follow(&path)?;
    let capacity =
        usize::try_from(metadata.len()).context("solver manifest byte length exceeds usize")?;
    let mut bytes = Vec::with_capacity(capacity);
    file.by_ref()
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read solver manifest {}", path.display()))?;
    let bytes_read =
        u64::try_from(bytes.len()).context("solver manifest byte count exceeds u64")?;
    ensure!(
        bytes_read == metadata.len(),
        "solver manifest changed while it was read: {}",
        path.display()
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&bytes).context("solver manifest is not valid JSON")?;
    manifest
        .get("build")
        .and_then(|build| build.get("target_triple"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .context("solver manifest has no build.target_triple")
}

fn copy_artifact_tree(artifact_root: &Path, staging: &Path, target: SidecarTarget) -> Result<()> {
    let metadata = fs::symlink_metadata(artifact_root).with_context(|| {
        format!(
            "failed to inspect solver artifact root {}",
            artifact_root.display()
        )
    })?;
    ensure!(
        metadata.is_dir() && !is_link_like(&metadata),
        "solver artifact root must be a regular directory: {}",
        artifact_root.display()
    );
    fs::create_dir(staging).with_context(|| {
        format!(
            "failed to create private sidecar staging directory {}",
            staging.display()
        )
    })?;
    let bin = staging.join("bin");
    let resources = staging.join("resources");
    fs::create_dir(&bin)
        .with_context(|| format!("failed to create sidecar bin directory {}", bin.display()))?;
    fs::create_dir(&resources).with_context(|| {
        format!(
            "failed to create sidecar resources directory {}",
            resources.display()
        )
    })?;

    let mut state = CopyState::default();
    copy_artifact_directory(
        artifact_root,
        Path::new(""),
        &bin,
        &resources,
        target,
        &mut state,
    )?;
    ensure!(
        state.worker_found,
        "validated solver artifact is missing {}",
        target.artifact_worker
    );
    Ok(())
}

fn validate_staged_artifact(
    repository: &Path,
    staging: &Path,
    target: SidecarTarget,
) -> Result<String> {
    let resources = staging.join("resources");
    let staged_worker = staging.join("bin").join(target.bundled_worker);
    let worker_metadata = fs::symlink_metadata(&staged_worker).with_context(|| {
        format!(
            "failed to inspect staged sidecar worker {}",
            staged_worker.display()
        )
    })?;
    ensure!(
        worker_metadata.is_file() && !is_link_like(&worker_metadata),
        "staged sidecar worker must be a regular file: {}",
        staged_worker.display()
    );
    let transient_worker = resources.join(target.artifact_worker);
    copy_regular_file(
        &staged_worker,
        &resources,
        &transient_worker,
        &worker_metadata,
        true,
    )?;
    let validation = crate::solver_manifest::validate(crate::solver_manifest::ValidateOptions {
        source_contract: &repository.join("workers/ortools/source-contract.json"),
        protocol_schema: &repository.join("protocol/solver-worker.proto"),
        protocol_policy: &repository.join("protocol/version.json"),
        artifact_root: &resources,
    });
    let cleanup = remove_path_no_follow(&transient_worker)
        .context("failed to remove transient staged worker after manifest validation");
    let manifest_sha256 = validation?;
    cleanup?;
    Ok(manifest_sha256)
}

#[derive(Default)]
struct CopyState {
    entries: usize,
    bytes: u64,
    worker_found: bool,
}

fn copy_artifact_directory(
    artifact_root: &Path,
    relative_directory: &Path,
    bin: &Path,
    resources: &Path,
    target: SidecarTarget,
    state: &mut CopyState,
) -> Result<()> {
    let source_directory = artifact_root.join(relative_directory);
    let entries = fs::read_dir(&source_directory).with_context(|| {
        format!(
            "failed to read solver artifact directory {}",
            source_directory.display()
        )
    })?;
    for entry in entries {
        let entry = entry.with_context(|| {
            format!(
                "failed to read an entry in solver artifact directory {}",
                source_directory.display()
            )
        })?;
        state.entries = state
            .entries
            .checked_add(1)
            .context("solver artifact entry count overflow")?;
        ensure!(
            state.entries <= MAX_SIDECAR_TREE_ENTRIES,
            "solver artifact tree exceeds {MAX_SIDECAR_TREE_ENTRIES} entries"
        );
        let relative = relative_directory.join(entry.file_name());
        ensure_safe_relative_path(&relative)?;
        let source = artifact_root.join(&relative);
        let metadata = fs::symlink_metadata(&source)
            .with_context(|| format!("failed to inspect solver artifact {}", source.display()))?;
        ensure!(
            !is_link_like(&metadata),
            "solver artifact contains a link or reparse point: {}",
            relative.display()
        );
        if metadata.is_dir() {
            copy_artifact_directory(artifact_root, &relative, bin, resources, target, state)?;
        } else if metadata.is_file() {
            ensure!(
                metadata.len() <= MAX_SIDECAR_FILE_BYTES,
                "solver artifact file exceeds the sidecar limit: {}",
                relative.display()
            );
            state.bytes = state
                .bytes
                .checked_add(metadata.len())
                .context("solver artifact byte count overflow")?;
            ensure!(
                state.bytes <= MAX_SIDECAR_TOTAL_BYTES,
                "solver artifact exceeds the total sidecar byte limit"
            );
            let (destination_root, destination, executable) =
                if relative == Path::new(target.artifact_worker) {
                    ensure!(
                        !state.worker_found,
                        "solver artifact contains duplicate worker entries"
                    );
                    state.worker_found = true;
                    (bin, bin.join(target.bundled_worker), true)
                } else {
                    (resources, resources.join(&relative), false)
                };
            copy_regular_file(
                &source,
                destination_root,
                &destination,
                &metadata,
                executable,
            )?;
        } else {
            bail!(
                "solver artifact contains an unsafe filesystem entry: {}",
                relative.display()
            );
        }
    }
    Ok(())
}

fn ensure_safe_relative_path(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "solver artifact contains an unsafe relative path: {}",
        path.display()
    );
    Ok(())
}

fn copy_regular_file(
    source: &Path,
    destination_root: &Path,
    destination: &Path,
    metadata: &fs::Metadata,
    executable: bool,
) -> Result<()> {
    ensure_destination_parent(destination_root, destination)?;
    let input = open_regular_file_no_follow(source)?;
    let opened = input
        .metadata()
        .with_context(|| format!("failed to inspect opened artifact {}", source.display()))?;
    ensure!(
        opened.is_file() && !is_link_like(&opened) && opened.len() == metadata.len(),
        "solver artifact changed while it was opened: {}",
        source.display()
    );
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .with_context(|| {
            format!(
                "failed to create staged sidecar file {}",
                destination.display()
            )
        })?;
    let copied = io::copy(&mut input.take(metadata.len() + 1), &mut output).with_context(|| {
        format!(
            "failed to copy solver artifact {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    ensure!(
        copied == metadata.len(),
        "solver artifact changed while it was copied: {}",
        source.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = if executable { 0o755 } else { 0o644 };
        fs::set_permissions(destination, fs::Permissions::from_mode(mode)).with_context(|| {
            format!(
                "failed to set staged sidecar file permissions {}",
                destination.display()
            )
        })?;
    }
    #[cfg(not(unix))]
    {
        let _ = executable;
        fs::set_permissions(destination, metadata.permissions()).with_context(|| {
            format!(
                "failed to preserve staged sidecar file permissions {}",
                destination.display()
            )
        })?;
    }
    Ok(())
}

fn ensure_destination_parent(destination_root: &Path, destination: &Path) -> Result<()> {
    let root_metadata = fs::symlink_metadata(destination_root).with_context(|| {
        format!(
            "failed to inspect staged sidecar root {}",
            destination_root.display()
        )
    })?;
    ensure!(
        root_metadata.is_dir() && !is_link_like(&root_metadata),
        "staged sidecar root is unsafe: {}",
        destination_root.display()
    );
    let parent = destination
        .parent()
        .context("staged sidecar file has no parent")?;
    let relative = parent
        .strip_prefix(destination_root)
        .context("staged sidecar parent is outside the private staging tree")?;
    let mut directory = destination_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            bail!("staged sidecar parent contains an unsafe component");
        };
        directory.push(component);
        let metadata = match fs::symlink_metadata(&directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&directory).with_context(|| {
                    format!(
                        "failed to create staged sidecar directory {}",
                        directory.display()
                    )
                })?;
                fs::symlink_metadata(&directory)?
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect staged sidecar directory {}",
                        directory.display()
                    )
                });
            }
        };
        ensure!(
            metadata.is_dir() && !is_link_like(&metadata),
            "staged sidecar path component is unsafe: {}",
            directory.display()
        );
    }
    Ok(())
}

fn add_no_follow_flags(options: &mut OpenOptions) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        let no_follow = i32::try_from(rustix::fs::OFlags::NOFOLLOW.bits())
            .context("platform O_NOFOLLOW flag does not fit i32")?;
        options.custom_flags(no_follow);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    Ok(())
}

fn open_regular_file_no_follow(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    add_no_follow_flags(&mut options)?;
    options
        .open(path)
        .with_context(|| format!("failed to open regular artifact file {}", path.display()))
}

#[derive(Debug)]
struct SidecarStageGuard {
    parent: PathBuf,
    lock: Option<File>,
    committed: bool,
}

impl SidecarStageGuard {
    fn acquire(repository: &Path, parent: &Path) -> Result<Self> {
        create_private_directory_chain(repository, parent)?;
        let lock_path = parent.join(SIDECAR_LOCK);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        add_no_follow_flags(&mut options)?;
        let lock = options.open(&lock_path).with_context(|| {
            format!(
                "failed to open solver sidecar staging lock {}",
                lock_path.display()
            )
        })?;
        let metadata = lock.metadata().context("failed to inspect sidecar lock")?;
        ensure!(
            metadata.is_file() && !is_link_like(&metadata),
            "solver sidecar staging lock must be a direct regular file"
        );
        lock.try_lock().with_context(|| {
            format!(
                "solver sidecar staging is already locked at {}",
                lock_path.display()
            )
        })?;
        Ok(Self {
            parent: parent.to_path_buf(),
            lock: Some(lock),
            committed: false,
        })
    }

    fn staging(&self) -> PathBuf {
        self.parent.join(SIDECAR_STAGING)
    }

    fn previous(&self) -> PathBuf {
        self.parent.join(SIDECAR_PREVIOUS)
    }

    fn prepare(&self) -> Result<()> {
        let current = self.parent.join("sidecar");
        let previous = self.previous();
        recover_publication(&current, &previous)?;
        remove_path_no_follow(&self.staging())
    }

    fn commit(&mut self) {
        drop(self.lock.take());
        self.committed = true;
    }
}

impl Drop for SidecarStageGuard {
    fn drop(&mut self) {
        if !self.committed {
            let _ = remove_path_no_follow(&self.staging());
        }
        drop(self.lock.take());
    }
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
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
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
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
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
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
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
    let text = std::str::from_utf8(&bytes).context("compiler version authority is not UTF-8")?;
    let value = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
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
    let parent = current
        .parent()
        .context("native worker publication has no parent")?;
    publish_directory(staging, current, &parent.join("previous"))
}

fn publish_directory(staging: &Path, current: &Path, previous: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(staging)
        .with_context(|| format!("staging directory is missing: {}", staging.display()))?;
    ensure!(
        metadata.is_dir() && !is_link_like(&metadata),
        "staging path must be a regular directory: {}",
        staging.display()
    );
    ensure!(
        !path_exists_no_follow(previous)?,
        "publication backup already exists: {}",
        previous.display()
    );
    let had_current = path_exists_no_follow(current)?;
    if had_current {
        let current_metadata = fs::symlink_metadata(current)?;
        ensure!(
            current_metadata.is_dir() && !is_link_like(&current_metadata),
            "current publication must be a regular directory: {}",
            current.display()
        );
        fs::rename(current, previous).context("failed to move the previous publication aside")?;
    }
    if let Err(error) = fs::rename(staging, current) {
        if had_current {
            fs::rename(previous, current).context("failed to restore the previous publication")?;
        }
        return Err(error).with_context(|| {
            format!(
                "failed to atomically publish {} as {}",
                staging.display(),
                current.display()
            )
        });
    }
    if had_current {
        remove_path_no_follow(previous)
            .context("failed to remove the previous publication backup")?;
    }
    Ok(())
}

fn recover_publication(current: &Path, previous: &Path) -> Result<()> {
    let current_exists = path_exists_no_follow(current)?;
    let previous_exists = path_exists_no_follow(previous)?;
    if current_exists {
        let metadata = fs::symlink_metadata(current)?;
        ensure!(
            metadata.is_dir() && !is_link_like(&metadata),
            "current publication must be a regular directory: {}",
            current.display()
        );
    }
    if previous_exists {
        let metadata = fs::symlink_metadata(previous)?;
        ensure!(
            metadata.is_dir() && !is_link_like(&metadata),
            "publication backup must be a regular directory: {}",
            previous.display()
        );
    }
    match (current_exists, previous_exists) {
        (false, true) => {
            fs::rename(previous, current).context("failed to recover the previous publication")
        }
        (true, true) => remove_path_no_follow(previous)
            .context("failed to remove an obsolete publication backup"),
        _ => Ok(()),
    }
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

    #[cfg(not(windows))]
    use super::verify_current_nix_result;
    use super::{
        NativeBuildGuard, SIDECAR_LOCK, SIDECAR_PREVIOUS, SIDECAR_STAGING, SidecarStageGuard,
        copy_artifact_tree, publish_staging, read_compiler_version, require_native_target,
        sidecar_target, verify_generated_build_input,
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
    fn maps_only_supported_targets_to_tauri_sidecar_names() {
        let cases = [
            (
                "x86_64-pc-windows-msvc",
                "bin/ortools-worker.exe",
                "ortools-worker-x86_64-pc-windows-msvc.exe",
            ),
            (
                "x86_64-apple-darwin",
                "bin/ortools-worker",
                "ortools-worker-x86_64-apple-darwin",
            ),
            (
                "aarch64-apple-darwin",
                "bin/ortools-worker",
                "ortools-worker-aarch64-apple-darwin",
            ),
            (
                "x86_64-unknown-linux-gnu",
                "bin/ortools-worker",
                "ortools-worker-x86_64-unknown-linux-gnu",
            ),
        ];
        for (triple, artifact_worker, bundled_worker) in cases {
            let target = sidecar_target(triple).unwrap();
            assert_eq!(target.artifact_worker, artifact_worker);
            assert_eq!(target.bundled_worker, bundled_worker);
        }
        assert!(sidecar_target("aarch64-unknown-linux-gnu").is_err());
        assert!(sidecar_target("x86_64-pc-windows-gnu").is_err());
    }

    #[test]
    fn splits_worker_from_resource_tree_without_flattening_paths() {
        let root = TempDir::new().unwrap();
        let artifact = root.path().join("artifact");
        fs::create_dir_all(artifact.join("bin")).unwrap();
        fs::create_dir_all(artifact.join("lib")).unwrap();
        fs::create_dir_all(artifact.join("licenses")).unwrap();
        fs::write(artifact.join("bin/ortools-worker"), b"worker").unwrap();
        fs::write(artifact.join("lib/libprotobuf.so.33"), b"runtime").unwrap();
        fs::write(artifact.join("licenses/NOTICE.txt"), b"notice").unwrap();
        fs::write(artifact.join("solver-manifest.json"), b"manifest").unwrap();
        let staging = root.path().join("staging");

        copy_artifact_tree(
            &artifact,
            &staging,
            sidecar_target("x86_64-unknown-linux-gnu").unwrap(),
        )
        .unwrap();

        assert_eq!(
            fs::read(staging.join("bin/ortools-worker-x86_64-unknown-linux-gnu")).unwrap(),
            b"worker"
        );
        assert_eq!(
            fs::read(staging.join("resources/lib/libprotobuf.so.33")).unwrap(),
            b"runtime"
        );
        assert_eq!(
            fs::read(staging.join("resources/licenses/NOTICE.txt")).unwrap(),
            b"notice"
        );
        assert_eq!(
            fs::read(staging.join("resources/solver-manifest.json")).unwrap(),
            b"manifest"
        );
        assert!(!staging.join("resources/bin/ortools-worker").exists());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_links_in_the_validated_artifact_copy_boundary() {
        use std::os::unix::fs::symlink;

        let root = TempDir::new().unwrap();
        let artifact = root.path().join("artifact");
        fs::create_dir_all(artifact.join("bin")).unwrap();
        fs::write(artifact.join("bin/ortools-worker"), b"worker").unwrap();
        symlink(
            artifact.join("bin/ortools-worker"),
            artifact.join("solver-manifest.json"),
        )
        .unwrap();

        let error = copy_artifact_tree(
            &artifact,
            &root.path().join("staging"),
            sidecar_target("x86_64-unknown-linux-gnu").unwrap(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("link or reparse point"));
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

    #[cfg(unix)]
    #[test]
    fn current_nix_result_must_resolve_to_the_pinned_output() {
        use std::os::unix::fs::symlink;

        let repository = TempDir::new().unwrap();
        let expected = repository.path().join("store-expected");
        let other = repository.path().join("store-other");
        fs::create_dir(&expected).unwrap();
        fs::create_dir(&other).unwrap();
        let result = repository.path().join("result");

        symlink(&expected, &result).unwrap();
        verify_current_nix_result(repository.path(), &expected).unwrap();

        fs::remove_file(&result).unwrap();
        symlink(&other, &result).unwrap();
        let error = verify_current_nix_result(repository.path(), &expected).unwrap_err();
        assert!(error.to_string().contains("does not match"));
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

    #[test]
    fn sidecar_advisory_lock_is_exclusive_and_survives_owner_exit() {
        let repository = TempDir::new().unwrap();
        let parent = repository.path().join("apps/desktop/src-tauri");
        fs::create_dir_all(&parent).unwrap();
        let lock_path = parent.join(SIDECAR_LOCK);
        fs::write(&lock_path, b"stale owner metadata").unwrap();

        let first = SidecarStageGuard::acquire(repository.path(), &parent).unwrap();
        let error = SidecarStageGuard::acquire(repository.path(), &parent).unwrap_err();
        assert!(error.to_string().contains("already locked"));
        drop(first);

        assert!(lock_path.is_file());
        let _next = SidecarStageGuard::acquire(repository.path(), &parent).unwrap();
    }

    #[test]
    fn interrupted_sidecar_stage_recovers_last_complete_tree() {
        let repository = TempDir::new().unwrap();
        let parent = repository.path().join("apps/desktop/src-tauri");
        fs::create_dir_all(&parent).unwrap();
        let current = parent.join("sidecar");
        let previous = parent.join(SIDECAR_PREVIOUS);
        let staging = parent.join(SIDECAR_STAGING);
        fs::create_dir(&current).unwrap();
        fs::write(current.join("payload"), b"last-complete").unwrap();
        fs::rename(&current, &previous).unwrap();
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("payload"), b"partial").unwrap();

        let guard = SidecarStageGuard::acquire(repository.path(), &parent).unwrap();
        guard.prepare().unwrap();

        assert!(!previous.exists());
        assert!(!staging.exists());
        assert_eq!(fs::read(current.join("payload")).unwrap(), b"last-complete");
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
        fs::write(&version, b"19.51.36256.0\r\n").unwrap();
        assert_eq!(read_compiler_version(&version).unwrap(), "19.51.36256.0");

        fs::write(&version, b"19.44\nextra\n").unwrap();
        assert!(read_compiler_version(&version).is_err());
    }
}
