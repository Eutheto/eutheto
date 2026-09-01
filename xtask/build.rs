use std::env;
use std::error::Error;
use std::io;
use std::process::Command;
const PROTO: &str = "../protocol/solver-worker.proto";
const PROTO_ROOT: &str = "../protocol";

const REQUIRED_PROTOC_VERSION: &str = "libprotoc 33.1";
fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed={PROTO}");
    println!("cargo:rerun-if-env-changed=PROTOC");

    let protoc = env::var_os("PROTOC").ok_or_else(|| {
        io::Error::other("PROTOC must name the repository-pinned protoc 33.1 executable")
    })?;
    let version = Command::new(&protoc)
        .arg("--version")
        .output()
        .map_err(|source| {
            io::Error::other(format!(
                "failed to execute pinned protoc at {}: {source}",
                protoc.to_string_lossy()
            ))
        })?;
    let actual = String::from_utf8(version.stdout)?;
    if !version.status.success() || actual.trim() != REQUIRED_PROTOC_VERSION {
        return Err(io::Error::other(format!(
            "eutheto protocol generation requires exactly {REQUIRED_PROTOC_VERSION}; {} reported `{}`",
            protoc.to_string_lossy(),
            actual.trim()
        ))
        .into());
    }

    prost_build::Config::new()
        .bytes(["."])
        .compile_protos(&[PROTO], &[PROTO_ROOT])
        .map_err(|source| {
            io::Error::other(format!(
                "failed to compile {PROTO} with protoc; enter the repository's default Nix development shell or set PROTOC to the matched protoc executable: {source}"
            ))
        })?;

    Ok(())
}
