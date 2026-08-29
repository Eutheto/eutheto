//! Command handling for the explicitly non-final Phase-00 `optimizer` CLI.

use clap::{Args, Parser, Subcommand};
use eutheto_core::FoundationStatusService;
use std::ffi::OsString;
use std::io::{self, Write};
use thiserror::Error;

/// Errors produced while parsing or rendering CLI output.
#[derive(Debug, Error)]
pub enum CliError {
    /// Command-line arguments did not match the Phase-00 command surface.
    #[error(transparent)]
    Arguments(#[from] clap::Error),
    /// The stable JSON status could not be serialized.
    #[error("could not serialize foundation status")]
    Serialization(#[from] serde_json::Error),
    /// Status output could not be written to the caller.
    #[error("could not write foundation status")]
    Output(#[source] io::Error),
}

#[derive(Debug, Parser)]
#[command(
    name = "optimizer",
    version,
    about = "Non-final Phase-00 foundation CLI",
    long_about = "Reports the Phase-00 foundation shell only. This non-final CLI provides no domain or solver commands."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Report the capability of the compiled foundation shell.
    Status(StatusArgs),
}

#[derive(Args, Debug)]
struct StatusArgs {
    /// Emit the stable status DTO as compact JSON.
    #[arg(long)]
    json: bool,
}

/// Parses `args`, executes the selected command, and writes its observable output.
///
/// The caller owns argument and output streams, which keeps command execution
/// deterministic and makes write failures explicit.
///
/// # Errors
///
/// Returns [`CliError`] when arguments are invalid, JSON serialization fails,
/// or the output stream cannot be written.
pub fn run_from<I, T, W>(args: I, mut output: W) -> Result<(), CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    W: Write,
{
    let cli = Cli::try_parse_from(args)?;
    let service = FoundationStatusService::current();

    match cli.command {
        Command::Status(status_args) => write_status(&mut output, service, status_args.json),
    }
}

fn write_status(
    output: &mut impl Write,
    service: FoundationStatusService,
    json: bool,
) -> Result<(), CliError> {
    let status = service.status();

    if json {
        serde_json::to_writer(&mut *output, &status)?;
        writeln!(output).map_err(CliError::Output)?;
    } else {
        writeln!(output, "optimizer (non-final Phase-00 CLI)").map_err(CliError::Output)?;
        writeln!(
            output,
            "foundation: {} {}",
            service.package_name(),
            service.package_version()
        )
        .map_err(CliError::Output)?;
        writeln!(output, "schema version: {}", status.schema_version).map_err(CliError::Output)?;
        writeln!(output, "capability: {}", status.capability).map_err(CliError::Output)?;
    }

    Ok(())
}
