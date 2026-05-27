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
        /// Override the auto-generated test scaffold with a custom Solidity
        /// template file. The file must contain `{{CHECK_FN}}` and may
        /// reference `{{NAME}}`. When omitted, Vergil reads the first .sol
        /// under `<path>/src/`, extracts the contract identifier, and
        /// synthesizes a default scaffold importing it with empty
        /// constructor args. Provide this flag for contracts whose
        /// constructor takes non-default arguments.
        #[arg(long)]
        scaffold: Option<PathBuf>,
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
    Json,
}

fn main() -> ExitCode {
    // Subscribe to tracing with a default of `warn` so the user sees
    // synth/critique/dispatch warnings when something goes wrong. The
    // CLI is the right boundary to install the subscriber — library
    // crates emit events but never configure the global subscriber.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let cli = Cli::parse();
    let result: Result<(), u8> = match cli.command {
        Command::Verify {
            path,
            properties,
            format,
            intent,
            scaffold,
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
            rt.block_on(commands::verify::run(
                path, properties, format, intent, scaffold,
            ))
        }
        Command::Init => commands::init::run(),
        Command::Prove { artifact } => commands::prove::run(artifact),
        Command::Bench => commands::bench::run(),
        Command::Corpus { .. } => commands::corpus::run(),
        Command::Doctor => commands::doctor::run(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code),
    }
}
