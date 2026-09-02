mod architecture;
mod fixtures;
mod generate;
mod phase02_generate;
mod protocol;
mod protocol_generate;
mod release;
mod solver;
mod source_contract;
mod supply_chain;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "xtask",
    about = "Repository-owned deterministic maintenance tasks"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Regenerate checked-in deterministic sources.
    Generate,
    /// Verify that checked-in generated sources have no drift.
    GenerateCheck,
    /// Verify versioned worker-protocol assets.
    Protocol {
        #[command(subcommand)]
        command: ProtocolCommand,
    },
    /// Validate repository fixture discovery and filesystem safety.
    Fixtures {
        #[command(subcommand)]
        command: FixturesCommand,
    },
    /// Verify workspace dependency direction and Phase-02 crate boundaries.
    Architecture {
        #[command(subcommand)]
        command: ArchitectureCommand,
    },
    /// Native solver-worker build and deferred installation operations.
    Solver {
        #[command(subcommand)]
        command: SolverCommand,
    },
    /// Third-party license artifact operations.
    Licenses {
        #[command(subcommand)]
        command: GenerateCommand,
    },
    /// Software-bill-of-materials operations.
    Sbom {
        #[command(subcommand)]
        command: GenerateCommand,
    },
    /// Release verification and assembly operations.
    Release {
        #[command(subcommand)]
        command: ReleaseCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ArchitectureCommand {
    /// Reject missing Phase-02 crates and forbidden transitive dependency paths.
    Verify,
}

#[derive(Debug, Subcommand)]
enum ProtocolCommand {
    /// Verify every checked-in semantic-JSON/framed-protobuf fixture pair and declared limit.
    Verify,
}

#[derive(Debug, Subcommand)]
enum FixturesCommand {
    /// Discover fixture roots and reject empty files, symlinks, and invalid paths.
    Validate,
}

#[derive(Debug, Subcommand)]
enum SolverCommand {
    BuildNative,
    InstallFromNix,
    Smoke,
}

#[derive(Debug, Subcommand)]
enum GenerateCommand {
    Generate,
}

#[derive(Debug, Subcommand)]
enum ReleaseCommand {
    VerifyClean,
    AssembleManifest,
}

fn main() -> Result<()> {
    let root = repository_root()?;
    match Cli::parse().command {
        Command::Generate => generate::generate(&root),
        Command::GenerateCheck => {
            generate::check(&root)?;
            supply_chain::check_licenses(&root)?;
            supply_chain::check_sbom(&root)?;
            Ok(())
        }
        Command::Protocol {
            command: ProtocolCommand::Verify,
        } => protocol::verify(&root),
        Command::Fixtures {
            command: FixturesCommand::Validate,
        } => fixtures::validate(&root),
        Command::Architecture {
            command: ArchitectureCommand::Verify,
        } => architecture::verify(&root),
        Command::Solver { command } => match command {
            SolverCommand::BuildNative => solver::build_native(&root),
            SolverCommand::InstallFromNix => unavailable_solver(
                "install-from-nix",
                "the installed solver manifest and license payload contracts exist",
            ),
            SolverCommand::Smoke => unavailable_solver(
                "smoke",
                "manifest-validated worker installation is implemented",
            ),
        },
        Command::Licenses {
            command: GenerateCommand::Generate,
        } => supply_chain::generate_licenses(&root).map_err(anyhow::Error::from),
        Command::Sbom {
            command: GenerateCommand::Generate,
        } => supply_chain::generate_sbom(&root).map_err(anyhow::Error::from),
        Command::Release { command } => match command {
            ReleaseCommand::VerifyClean => release::verify_clean(&root),
            ReleaseCommand::AssembleManifest => release::assemble_manifest(),
        },
    }
}

fn repository_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .context("xtask manifest directory has no repository parent")
}

fn unavailable_solver(operation: &str, prerequisite: &str) -> Result<()> {
    bail!("solver {operation} is unavailable until {prerequisite}")
}
