use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

mod commands;

#[derive(Parser)]
#[command(
    name = "vergil",
    version,
    about = "Mathematically verified Solidity smart contracts"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Verify a Solidity contract against generated or supplied properties
    Verify {
        path: PathBuf,
        #[arg(long)]
        intent: Option<String>,
    },
    /// Scaffold a Vergil config in the current Foundry project
    Init,
    /// Re-check an existing proof artifact without LLM or solver search
    Prove { artifact: PathBuf },
    /// Run benchmark suites
    Bench,
    /// Manage the property catalog
    Corpus {
        #[command(subcommand)]
        action: CorpusAction,
    },
    /// Check that toolchain dependencies are installed
    Doctor,
}

#[derive(Subcommand)]
enum CorpusAction {
    /// Pull the latest property catalog
    Update,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Verify { .. } => commands::verify::run(),
        Command::Init => commands::init::run(),
        Command::Prove { .. } => commands::prove::run(),
        Command::Bench => commands::bench::run(),
        Command::Corpus { .. } => commands::corpus::run(),
        Command::Doctor => commands::doctor::run(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code),
    }
}
