use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process::ExitCode;

mod commands;
mod config;
mod output;

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
    /// Verify a Solidity project against a properties.yaml file
    Verify {
        /// Path to the Foundry project (the dir holding foundry.toml)
        path: PathBuf,
        /// Properties YAML file (defaults to <path>/properties.yaml)
        #[arg(long)]
        properties: Option<PathBuf>,
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        /// Natural-language intent (Phase 2; ignored in Phase 1)
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

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum OutputFormat {
    Text,
    Markdown,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result: Result<(), u8> = match cli.command {
        Command::Verify {
            path,
            properties,
            format,
            intent: _,
        } => {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("failed to build tokio runtime: {e}");
                    return ExitCode::from(3);
                }
            };
            rt.block_on(commands::verify::run(path, properties, format))
        }
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
