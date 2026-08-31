use serde_json::Value;
use std::error::Error;
use std::fs;
use std::process::Command;
use std::str;
fn private_tempdir() -> Result<tempfile::TempDir, Box<dyn Error>> {
    let base = dirs::home_dir().ok_or("platform home directory is unavailable")?;
    fs::create_dir_all(&base)?;
    Ok(tempfile::Builder::new()
        .prefix("eutheto-cli-status-test-")
        .tempdir_in(base)?)
}

#[test]
fn status_truthfully_reports_phase_01_capability() -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_optimizer"))
        .arg("status")
        .output()?;

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = str::from_utf8(&output.stdout)?;
    assert!(stdout.contains("optimizer (provisional development name)"));
    assert!(stdout.contains("capability: phase_01_core"));
    assert!(!stdout.contains("phase_00_foundation"));
    Ok(())
}

#[test]
fn legacy_status_json_uses_the_single_cli_result_envelope() -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_optimizer"))
        .args(["status", "--json"])
        .output()?;

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = str::from_utf8(&output.stdout)?;
    let Some(envelope) = stdout.strip_suffix('\n') else {
        return Err("JSON output must end with one newline".into());
    };
    assert!(
        !envelope.contains('\n'),
        "JSON output must contain exactly one envelope line"
    );
    let value: Value = serde_json::from_str(envelope)?;
    assert_eq!(value["apiVersion"], "eutheto/cli-result/v1");
    assert_eq!(value["command"], "status");
    assert_eq!(value["ok"], true);
    assert_eq!(value["result"]["schemaVersion"], 1);
    assert_eq!(value["result"]["capability"], "phase_01_core");
    Ok(())
}

#[test]
fn later_solver_command_is_catalogued_but_typed_unavailable() -> Result<(), Box<dyn Error>> {
    let directory = private_tempdir()?;
    let output = Command::new(env!("CARGO_BIN_EXE_optimizer"))
        .args(["--format", "json", "--data-dir"])
        .arg(directory.path())
        .args(["solve", "scenario.eutheto"])
        .output()?;

    assert_eq!(output.status.code(), Some(6));
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value["apiVersion"], "eutheto/cli-result/v1");
    assert_eq!(value["command"], "solve");
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "capability.solve_unavailable");
    assert!(!str::from_utf8(&output.stderr)?.contains("unrecognized subcommand"));
    Ok(())
}
