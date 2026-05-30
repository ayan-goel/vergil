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
    about = "LLM-guided formal verification for Solidity smart contracts",
    long_about = "Vergil verifies Solidity contracts via a portfolio of symbolic execution (Halmos) and CHC model checking (Solidity SMTChecker). The LLM proposes candidate properties from a natural-language intent; an independent critic rejects vacuous candidates; only the SMT solver decides correctness.\n\nExit codes (SPEC §3.1):\n  0  all properties verified\n  1  at least one counterexample\n  2  all resolved as unknown\n  3  pipeline error (toolchain, IO, config)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Verify a Solidity project against a properties.yaml file or a natural-language intent
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
        /// Stream structured telemetry events to a JSONL file (one event
        /// per line). V2's billing layer reads this file directly. Phase
        /// 4 Slice B2.
        #[arg(long, value_name = "PATH")]
        telemetry_json: Option<PathBuf>,
        /// Tenant identifier carried in every telemetry event. Default
        /// "internal" for in-house runs; V2 wires real per-customer IDs
        /// from the service-layer auth identity.
        #[arg(long, default_value = "internal")]
        tenant: String,
        /// Per-run cost ceiling in USD for the `--intent` (CEGIS) path.
        /// Overrides the default $10. The VergilBench sweep sets a tight
        /// value so a 100-contract run stays under its aggregate budget.
        #[arg(long)]
        cost_budget: Option<f64>,
        /// Synthesis fan-out (candidates per iteration) for the `--intent`
        /// path. Overrides the default 4. Higher values give the critic more
        /// candidates to accept, trading cost for verification yield (the
        /// kill-criterion sweep uses 16).
        #[arg(long)]
        samples: Option<usize>,
        /// Minimum per-axis critique score (vacuity / body-independence /
        /// testability) a candidate must clear to reach the solver, for the
        /// `--intent` path. Overrides the default 0.5; the kill-criterion
        /// sweep uses 0.4 (trading strictness for more candidates dispatched).
        #[arg(long)]
        min_critique_axis: Option<f32>,
        /// V1.5 zero-config tier. `zero-config` runs the attack-pattern
        /// catalog activation + self-tests against the project (Phase 1
        /// surface — per-contract dispatch lands in V1.5 Phase 4).
        /// `intent` keeps the V1 CEGIS path verbatim. Default: `intent`
        /// (preserves V1 behavior; SPEC §3.1's `both` default is the
        /// Phase 6 target once all four zero-config oracles land).
        #[arg(long, value_enum, default_value_t = VerifyMode::Intent)]
        mode: VerifyMode,
    },
    /// Scaffold a Vergil config in the current Foundry project (stub — see docs/book/src/cli-reference.md)
    Init,
    /// Re-check an existing proof.json without running Halmos again
    Prove {
        /// Path to a `proof.json` artifact emitted by a previous `vergil verify` run
        artifact: PathBuf,
        /// SMT solver to re-dispatch the captured queries through.
        /// Defaults to cvc5 (the alternate of Halmos's primary z3).
        /// Pass `--solver z3` or `--solver bitwuzla` to override.
        #[arg(long)]
        solver: Option<String>,
    },
    /// Run benchmark suites (stub — use the dedicated `vergilbench` binary)
    Bench,
    /// Manage the property catalog (stub — templates live in crates/vergil-properties/templates/)
    Corpus {
        #[command(subcommand)]
        action: CorpusAction,
    },
    /// Inspect the V1.5 attack-pattern catalog (templates/attacks/)
    Catalog {
        #[command(subcommand)]
        action: CatalogAction,
    },
    /// Check that toolchain dependencies (solc, halmos, forge, z3, cvc5, slither) are installed
    Doctor,
}

#[derive(Subcommand)]
enum CatalogAction {
    /// List loaded attack templates with id, severity, decidability, category
    List {
        /// Restrict to a single category (e.g. `access`, `reentrancy`, `arithmetic`)
        #[arg(long)]
        category: Option<String>,
    },
    /// Print the full manifest (English negation property, mitigation, references) for one attack
    Show {
        /// Attack-pattern id (snake-case, matches the template directory name)
        id: String,
    },
    /// Load every template and report any schema, lint, or missing-file errors. Non-zero exit on failure.
    Validate,
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

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
pub enum VerifyMode {
    /// V1.5 zero-config tier: run the attack-pattern catalog activation +
    /// per-template self-tests against the project. No LLM calls.
    ZeroConfig,
    /// V1 CEGIS path: synthesize properties from a `--intent` (or
    /// `properties.yaml`) and discharge via the SMT portfolio.
    Intent,
    /// Run both tiers and concatenate the results. (Phase-1 stub: in
    /// Phase 1 this is equivalent to running zero-config followed by
    /// intent; Phase 6 lands the stratified verdict combining them.)
    Both,
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
            telemetry_json,
            tenant,
            cost_budget,
            samples,
            min_critique_axis,
            mode,
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
            match mode {
                VerifyMode::ZeroConfig => rt.block_on(commands::zero_config::run(path)),
                VerifyMode::Intent => rt.block_on(commands::verify::run(
                    path,
                    properties,
                    format,
                    intent,
                    scaffold,
                    telemetry_json,
                    tenant,
                    cost_budget,
                    samples,
                    min_critique_axis,
                )),
                VerifyMode::Both => {
                    // Phase-1 simplification: run zero-config first, then
                    // intent if zero-config succeeded. Phase 6 lands the
                    // unified stratified verdict.
                    match rt.block_on(commands::zero_config::run(path.clone())) {
                        Ok(()) => rt.block_on(commands::verify::run(
                            path,
                            properties,
                            format,
                            intent,
                            scaffold,
                            telemetry_json,
                            tenant,
                            cost_budget,
                            samples,
                            min_critique_axis,
                        )),
                        Err(code) => Err(code),
                    }
                }
            }
        }
        Command::Init => commands::init::run(),
        Command::Prove { artifact, solver } => commands::prove::run_with_solver(artifact, solver),
        Command::Bench => commands::bench::run(),
        Command::Corpus { .. } => commands::corpus::run(),
        Command::Catalog { action } => match action {
            CatalogAction::List { category } => commands::catalog::run_list(category),
            CatalogAction::Show { id } => commands::catalog::run_show(id),
            CatalogAction::Validate => commands::catalog::run_validate(),
        },
        Command::Doctor => commands::doctor::run(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code),
    }
}
