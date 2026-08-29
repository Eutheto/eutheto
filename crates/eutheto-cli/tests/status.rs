use serde_json::json;
use std::error::Error;
use std::process::Command;
use std::str;

#[test]
fn status_prints_foundation_capability() -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_optimizer"))
        .arg("status")
        .output()?;

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = str::from_utf8(&output.stdout)?;
    assert_eq!(
        stdout,
        format!(
            "optimizer (non-final Phase-00 CLI)\nfoundation: eutheto-core {}\nschema version: 1\ncapability: phase_00_foundation\n",
            env!("CARGO_PKG_VERSION")
        )
    );
    Ok(())
}

#[test]
fn status_json_prints_only_stable_dto_fields() -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_optimizer"))
        .args(["status", "--json"])
        .output()?;

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        str::from_utf8(&output.stdout)?,
        "{\"schemaVersion\":1,\"capability\":\"phase_00_foundation\"}\n"
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        value,
        json!({
            "schemaVersion": 1,
            "capability": "phase_00_foundation"
        })
    );
    Ok(())
}

#[test]
fn solver_commands_are_not_claimed() -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_optimizer"))
        .arg("solve")
        .output()?;

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = str::from_utf8(&output.stderr)?;
    assert!(stderr.contains("unrecognized subcommand 'solve'"));
    Ok(())
}
