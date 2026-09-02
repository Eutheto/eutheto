use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, ensure};

const NATIVE_BUILD_SCRIPT: &str = "workers/ortools/cmake/native_windows_build.cmake";
const NATIVE_BUILD_ROOT: &str = ".cache/ortools-native/windows-x86_64";
const NATIVE_CURRENT: &str = ".cache/ortools-native/windows-x86_64/current";
const NATIVE_WORKER: &str = ".cache/ortools-native/windows-x86_64/current/bin/ortools-worker.exe";

pub(crate) fn build_native(repo_root: &Path) -> Result<()> {
    require_native_target(env::consts::OS, env::consts::ARCH)?;

    let repository = repo_root
        .canonicalize()
        .context("failed to resolve the repository root for the native worker build")?;
    let previous_result = repository.join(NATIVE_CURRENT);
    if previous_result.exists() {
        fs::remove_dir_all(&previous_result).with_context(|| {
            format!(
                "failed to invalidate the previous native worker result {}",
                previous_result.display()
            )
        })?;
    }

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
    let native_root_arg = cmake_path_argument("NATIVE_ROOT", &repository.join(NATIVE_BUILD_ROOT))?;
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

    let worker = repository.join(NATIVE_WORKER);
    ensure!(
        worker.is_file(),
        "native build reported success without the expected worker: {}",
        worker.display()
    );
    println!("built native Windows worker: {}", worker.display());
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
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::{require_native_target, verify_generated_build_input};

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
}
