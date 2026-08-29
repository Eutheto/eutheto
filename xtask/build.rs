use std::error::Error;
use std::io;

const PROTO: &str = "../protocol/solver-worker.proto";
const PROTO_ROOT: &str = "../protocol";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed={PROTO}");
    println!("cargo:rerun-if-env-changed=PROTOC");

    prost_build::Config::new()
        .compile_protos(&[PROTO], &[PROTO_ROOT])
        .map_err(|source| {
            io::Error::other(format!(
                "failed to compile {PROTO} with protoc; enter the repository's default Nix development shell or set PROTOC to the matched protoc executable: {source}"
            ))
        })?;

    Ok(())
}
