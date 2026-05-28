//! VergilBench runner — Phase 3 deliverable.
//!
//! Walks `vergilbench/contracts/<name>/`, runs the Vergil pipeline (Phase 1
//! deterministic path by default) against each contract's `properties.yaml`,
//! and writes the aggregate result to `vergilbench/results/<timestamp>.json`.
//!
//! Per `tasks/plan.md` Slice 11:
//!   - per-property budget: $0.50 (intent path; Phase 4)
//!   - aggregate budget: $200 (intent path; Phase 4)
//!   - results go to `vergilbench/results/<timestamp>.json`
//!   - scoreboard written to `benchmarks/results/README.md`
//!
//! The Phase 3 seed corpus uses the Phase 1 deterministic path (zero LLM
//! cost) since the corpus contracts are the existing examples/ references
//! with hand-written `check_` functions. Phase 4 will switch to the intent
//! path once the corpus grows to 100 contracts.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use clap::Parser;
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(
    name = "vergilbench",
    about = "Run the Vergil verifier against the bench corpus",
    long_about = None,
)]
struct Cli {
    /// Path to the VergilBench root (contains contracts/, expected/, results/).
    #[arg(long, default_value = "vergilbench")]
    corpus: PathBuf,
    /// Maximum number of contracts to run (for smoke tests). When None,
    /// runs every contract in `<corpus>/contracts/`.
    #[arg(long)]
    max: Option<usize>,
    /// Path to the `vergil` binary. Defaults to `./target/release/vergil`.
    #[arg(long, default_value = "./target/release/vergil")]
    vergil: PathBuf,
    /// Print verbose per-contract output to stderr.
    #[arg(long, default_value_t = false)]
    verbose: bool,
    /// Run the LLM-driven intent (CEGIS) path instead of the $0 deterministic
    /// path. Each contract's `intent:` field from properties.yaml drives a
    /// `vergil verify --intent` run. Phase 4 Slice A9.
    #[arg(long, default_value_t = false)]
    intent: bool,
    /// Aggregate USD budget for the intent sweep. The sweep aborts gracefully
    /// (recording the contracts covered) once cumulative cost reaches this.
    #[arg(long, default_value_t = 200.0)]
    aggregate_budget: f64,
    /// Per-contract USD ceiling forwarded to `vergil verify --cost-budget`.
    #[arg(long, default_value_t = 2.5)]
    per_contract_budget: f64,
    /// Synthesis fan-out forwarded to `vergil verify --samples` in intent mode.
    /// Higher gives the critic more candidates (more verification yield) at
    /// higher cost; the kill-criterion sweep uses 16.
    #[arg(long, default_value_t = 8)]
    samples: usize,
    /// Minimum per-axis critique score forwarded to
    /// `vergil verify --min-critique-axis` in intent mode. Defaults to the
    /// kill-criterion's 0.4 (the CLI interactive default is 0.5).
    #[arg(long, default_value_t = 0.4)]
    min_critique_axis: f32,
}

#[derive(Debug, Serialize)]
struct ContractResult {
    contract: String,
    verified: u32,
    total: u32,
    wall_clock_ms: u64,
    exit_code: i32,
    /// USD cost of this contract's run (0.0 on the deterministic path).
    #[serde(default)]
    cost_usd: f64,
}

#[derive(Debug, Serialize)]
struct AggregateResult {
    started_at: String,
    finished_at: String,
    per_contract: Vec<ContractResult>,
    /// Sum of verified across all contracts.
    verified_total: u32,
    /// Sum of properties across all contracts.
    property_total: u32,
    /// Overall pass rate.
    pass_rate: f64,
    total_wall_clock_ms: u64,
    aborted_on_budget: bool,
    /// "deterministic" ($0 template-match) or "intent" (LLM CEGIS sweep).
    mode: String,
    /// Aggregate USD cost across all contracts (0.0 on the deterministic path).
    total_cost_usd: f64,
    /// Set when the sweep halted early on a provider credit/quota error.
    halted_on_credit_error: bool,
}

fn main() {
    let args = Cli::parse();
    if let Err(e) = run(&args) {
        eprintln!("vergilbench: {e}");
        std::process::exit(1);
    }
}

fn run(args: &Cli) -> Result<(), String> {
    let contracts_dir = args.corpus.join("contracts");
    if !contracts_dir.is_dir() {
        return Err(format!(
            "no contracts directory at {}",
            contracts_dir.display()
        ));
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&contracts_dir)
        .map_err(|e| format!("read {}: {e}", contracts_dir.display()))?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    entries.sort();
    if let Some(max) = args.max {
        entries.truncate(max);
    }

    if entries.is_empty() {
        return Err("no contracts found in corpus".to_string());
    }

    eprintln!(
        "[vergilbench] running {} contracts from {}",
        entries.len(),
        contracts_dir.display()
    );

    if !args.vergil.is_file() {
        return Err(format!(
            "vergil binary not found at {} — build with `cargo build --release --bin vergil`",
            args.vergil.display()
        ));
    }

    let started = chrono::Utc::now();
    let mut per_contract = Vec::new();
    let started_instant = Instant::now();
    let mut total_cost = 0.0_f64;
    let mut aborted_on_budget = false;
    let mut halted_on_credit_error = false;

    for path in &entries {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>")
            .to_string();

        // Aggregate-budget gate (intent mode): refuse to start a run that
        // could push cumulative cost past the ceiling.
        if args.intent && total_cost + args.per_contract_budget > args.aggregate_budget {
            eprintln!(
                "[vergilbench] aggregate budget ${:.2} reached (spent ${:.2}); stopping after {} contracts",
                args.aggregate_budget,
                total_cost,
                per_contract.len()
            );
            aborted_on_budget = true;
            break;
        }

        eprintln!("[vergilbench] >>> {name}");
        let prop_count = count_properties(path).unwrap_or(0);

        let mut cmd = Command::new(&args.vergil);
        cmd.arg("verify").arg(path);
        if args.intent {
            match read_intent(path) {
                Some(intent) => {
                    cmd.arg("--intent")
                        .arg(intent)
                        .arg("--cost-budget")
                        .arg(format!("{}", args.per_contract_budget))
                        .arg("--samples")
                        .arg(format!("{}", args.samples))
                        .arg("--min-critique-axis")
                        .arg(format!("{}", args.min_critique_axis));
                }
                None => {
                    eprintln!("[vergilbench]     SKIP {name}: no `intent:` in properties.yaml");
                    per_contract.push(ContractResult {
                        contract: name,
                        verified: 0,
                        total: prop_count,
                        wall_clock_ms: 0,
                        exit_code: -1,
                        cost_usd: 0.0,
                    });
                    continue;
                }
            }
        }

        let one_started = Instant::now();
        let output = cmd.output().map_err(|e| format!("spawn vergil: {e}"))?;
        let wall = one_started.elapsed().as_millis() as u64;
        let exit = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Credit-exhaustion halt: never grind against a dead provider.
        if args.intent && exit != 0 && looks_like_credit_error(&stderr) {
            eprintln!(
                "[vergilbench] HALT: provider credit/quota error on {name}; stopping the sweep.\nstderr tail:\n{}",
                stderr.lines().rev().take(3).collect::<Vec<_>>().join("\n")
            );
            halted_on_credit_error = true;
            break;
        }

        let (verified, cost) = if args.intent {
            (
                parse_intent_verified(&stdout).unwrap_or(0),
                parse_cost(&stdout).unwrap_or(0.0),
            )
        } else {
            // The `Summary: N verified, ...` line of the deterministic path.
            (parse_verified_count(&stdout).unwrap_or(0), 0.0)
        };
        total_cost += cost;

        eprintln!(
            "[vergilbench]     verified {verified}/{prop_count} exit={exit} wall={wall}ms cost=${cost:.4} (cum ${total_cost:.2})"
        );
        if args.verbose {
            eprintln!("--- stdout ---\n{stdout}");
        }
        per_contract.push(ContractResult {
            contract: name,
            verified,
            total: prop_count,
            wall_clock_ms: wall,
            exit_code: exit,
            cost_usd: cost,
        });
    }

    let finished = chrono::Utc::now();
    let verified_total: u32 = per_contract.iter().map(|c| c.verified).sum();
    let property_total: u32 = per_contract.iter().map(|c| c.total).sum();
    // Cap per-contract verified at its property count so the intent path
    // (which can synthesize more candidates than ground-truth properties)
    // cannot push the aggregate pass-rate above 100%.
    let capped_verified: u32 = per_contract.iter().map(|c| c.verified.min(c.total)).sum();
    let pass_rate = if property_total == 0 {
        0.0
    } else {
        capped_verified as f64 / property_total as f64
    };
    let agg = AggregateResult {
        started_at: started.to_rfc3339(),
        finished_at: finished.to_rfc3339(),
        per_contract,
        verified_total,
        property_total,
        pass_rate,
        total_wall_clock_ms: started_instant.elapsed().as_millis() as u64,
        aborted_on_budget,
        mode: if args.intent {
            "intent".to_string()
        } else {
            "deterministic".to_string()
        },
        total_cost_usd: total_cost,
        halted_on_credit_error,
    };

    let results_dir = args.corpus.join("results");
    std::fs::create_dir_all(&results_dir).map_err(|e| format!("mkdir results: {e}"))?;
    let ts = started.format("%Y-%m-%dT%H-%M-%S-%6f%z").to_string();
    let out = results_dir.join(format!("{ts}.json"));
    let body = serde_json::to_string_pretty(&agg).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(&out, body).map_err(|e| format!("write {}: {e}", out.display()))?;
    eprintln!("[vergilbench] wrote {}", out.display());

    // Write scoreboard (Phase 3 SPEC §11.3 step 11): writes locally, no git push.
    write_scoreboard(&agg)?;

    eprintln!(
        "[vergilbench] DONE  verified {verified_total}/{property_total}  pass_rate={:.1}%  wall={total_ms}ms",
        pass_rate * 100.0,
        total_ms = agg.total_wall_clock_ms
    );
    Ok(())
}

fn count_properties(contract_dir: &Path) -> Option<u32> {
    let yaml = contract_dir.join("properties.yaml");
    let body = std::fs::read_to_string(&yaml).ok()?;
    // Crude count: each `  - name:` line at column 2 is one property.
    let n = body.lines().filter(|l| l.starts_with("  - name:")).count();
    Some(n as u32)
}

fn parse_verified_count(stdout: &str) -> Option<u32> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        let stripped = strip_ansi(trimmed);
        if let Some(rest) = stripped.strip_prefix("Summary: ") {
            if let Some(num) = rest.split_whitespace().next() {
                return num.parse().ok();
            }
        }
    }
    None
}

/// Read the top-level `intent:` field from a contract's properties.yaml.
/// The intent drives the `vergil verify --intent` (CEGIS) path.
fn read_intent(contract_dir: &Path) -> Option<String> {
    let body = std::fs::read_to_string(contract_dir.join("properties.yaml")).ok()?;
    for line in body.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("intent:") {
            let v = rest.trim();
            let v = v.strip_prefix('"').unwrap_or(v);
            let v = v.strip_suffix('"').unwrap_or(v);
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Parse the `verified: N` line emitted by the intent (CEGIS) path. Distinct
/// from the deterministic path's `Summary: N verified` line.
fn parse_intent_verified(stdout: &str) -> Option<u32> {
    for line in stdout.lines() {
        let s = strip_ansi(line.trim());
        if let Some(rest) = s.strip_prefix("verified: ") {
            return rest.trim().parse().ok();
        }
    }
    None
}

/// Parse the `cost: $X.XXXX (...)` line emitted by the intent path.
fn parse_cost(stdout: &str) -> Option<f64> {
    for line in stdout.lines() {
        let s = strip_ansi(line.trim());
        if let Some(rest) = s.strip_prefix("cost: $") {
            let num: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            return num.parse().ok();
        }
    }
    None
}

/// Heuristic for provider credit/quota exhaustion in a failed run's stderr.
/// Per the project rule, the sweep HALTS rather than grinding against a dead
/// provider or silently switching providers.
fn looks_like_credit_error(stderr: &str) -> bool {
    let l = stderr.to_lowercase();
    [
        "insufficient",
        "quota",
        "payment_required",
        "payment required",
        "out of credit",
        "credit balance",
        "billing",
        "rate limit",
        "429",
    ]
    .iter()
    .any(|needle| l.contains(needle))
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        if chars.peek() != Some(&'[') {
            continue;
        }
        chars.next();
        for next in chars.by_ref() {
            if next.is_ascii_alphabetic() {
                break;
            }
        }
    }
    out
}

fn write_scoreboard(agg: &AggregateResult) -> Result<(), String> {
    let sb_dir = std::path::Path::new("benchmarks/results");
    std::fs::create_dir_all(sb_dir).map_err(|e| format!("mkdir scoreboard: {e}"))?;
    let mut md = String::new();
    md.push_str("# VergilBench scoreboard\n\n");
    md.push_str(&format!(
        "Last run: **{}** — mode **{}** — verified **{}/{}** ({:.1}%) in {:.2}s — cost **${:.2}**\n\n",
        agg.finished_at,
        agg.mode,
        agg.verified_total,
        agg.property_total,
        agg.pass_rate * 100.0,
        agg.total_wall_clock_ms as f64 / 1000.0,
        agg.total_cost_usd,
    ));
    if agg.aborted_on_budget {
        md.push_str("> ⚠️ Sweep aborted on aggregate budget — coverage is partial.\n\n");
    }
    if agg.halted_on_credit_error {
        md.push_str(
            "> ⛔ Sweep HALTED on a provider credit/quota error — coverage is partial.\n\n",
        );
    }
    md.push_str("| Contract | Verified | Total | Wall clock | Cost | Exit |\n");
    md.push_str("|---|---|---|---|---|---|\n");
    for c in &agg.per_contract {
        md.push_str(&format!(
            "| `{}` | {} | {} | {} ms | ${:.4} | {} |\n",
            c.contract, c.verified, c.total, c.wall_clock_ms, c.cost_usd, c.exit_code,
        ));
    }
    let out = sb_dir.join("README.md");
    std::fs::write(&out, md).map_err(|e| format!("write {}: {e}", out.display()))?;
    eprintln!("[vergilbench] wrote scoreboard {}", out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_color_codes() {
        let input = "\x1b[32m[PASS]\x1b[0m foo";
        assert_eq!(strip_ansi(input), "[PASS] foo");
    }

    #[test]
    fn parse_verified_count_handles_summary_line() {
        let stdout = "\n  ✓ p1 — verified\n\nSummary: \x1b[32m4 verified\x1b[0m, 0 counterexample, 0 unknown, 0 error\n";
        assert_eq!(parse_verified_count(stdout), Some(4));
    }

    #[test]
    fn parse_verified_count_returns_none_for_no_summary() {
        assert_eq!(parse_verified_count("no summary here"), None);
    }

    #[test]
    fn parse_intent_verified_reads_verified_line() {
        let out = "intent: foo\niterations: 2\nsynthesized: 5 candidates\nverified: 3\n  ✓ check_x\ncost: $0.42 (1/2 tokens)\n";
        assert_eq!(parse_intent_verified(out), Some(3));
        assert_eq!(parse_intent_verified("synthesized: 5 candidates\n"), None);
    }

    #[test]
    fn parse_cost_reads_dollar_amount() {
        let out = "verified: 2\ncost: $1.2345 (100/200 tokens)\n";
        assert_eq!(parse_cost(out), Some(1.2345));
        assert_eq!(parse_cost("no cost here"), None);
    }

    #[test]
    fn parse_cost_handles_ansi_prefix() {
        let out = "\x1b[32mcost: $0.5000\x1b[0m (1/1 tokens)";
        assert_eq!(parse_cost(out), Some(0.5));
    }

    #[test]
    fn credit_error_detection_flags_exhaustion_not_timeouts() {
        assert!(looks_like_credit_error(
            "Error: insufficient_quota for this key"
        ));
        assert!(looks_like_credit_error("provider returned HTTP 429"));
        assert!(looks_like_credit_error("payment required (402)"));
        assert!(looks_like_credit_error("Your credit balance is too low"));
        assert!(!looks_like_credit_error("halmos timed out after 120s"));
        assert!(!looks_like_credit_error(
            "intent run failed: synthesis produced no candidates"
        ));
    }
}
