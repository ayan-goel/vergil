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
}

#[derive(Debug, Serialize)]
struct ContractResult {
    contract: String,
    verified: u32,
    total: u32,
    wall_clock_ms: u64,
    exit_code: i32,
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

    for path in &entries {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>")
            .to_string();
        eprintln!("[vergilbench] >>> {name}");
        let prop_count = count_properties(path).unwrap_or(0);
        let one_started = Instant::now();
        let output = Command::new(&args.vergil)
            .arg("verify")
            .arg(path)
            .output()
            .map_err(|e| format!("spawn vergil: {e}"))?;
        let wall = one_started.elapsed().as_millis() as u64;
        let exit = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // The `Summary: N verified, M counterexample, ...` line gives us
        // the verified count without re-parsing JSON.
        let verified = parse_verified_count(&stdout).unwrap_or(0);
        eprintln!("[vergilbench]     verified {verified}/{prop_count} exit={exit} wall={wall}ms");
        if args.verbose {
            eprintln!("--- stdout ---\n{stdout}");
        }
        per_contract.push(ContractResult {
            contract: name,
            verified,
            total: prop_count,
            wall_clock_ms: wall,
            exit_code: exit,
        });
    }

    let finished = chrono::Utc::now();
    let verified_total: u32 = per_contract.iter().map(|c| c.verified).sum();
    let property_total: u32 = per_contract.iter().map(|c| c.total).sum();
    let pass_rate = if property_total == 0 {
        0.0
    } else {
        verified_total as f64 / property_total as f64
    };
    let agg = AggregateResult {
        started_at: started.to_rfc3339(),
        finished_at: finished.to_rfc3339(),
        per_contract,
        verified_total,
        property_total,
        pass_rate,
        total_wall_clock_ms: started_instant.elapsed().as_millis() as u64,
        aborted_on_budget: false,
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
        "Last run: **{}** — verified **{}/{}** ({:.1}%) in {:.2}s\n\n",
        agg.finished_at,
        agg.verified_total,
        agg.property_total,
        agg.pass_rate * 100.0,
        agg.total_wall_clock_ms as f64 / 1000.0,
    ));
    md.push_str("| Contract | Verified | Total | Wall clock | Exit |\n");
    md.push_str("|---|---|---|---|---|\n");
    for c in &agg.per_contract {
        md.push_str(&format!(
            "| `{}` | {} | {} | {} ms | {} |\n",
            c.contract, c.verified, c.total, c.wall_clock_ms, c.exit_code,
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
}
