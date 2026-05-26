//! Calibration runner. Walks `vergilbench/calibration/contracts/<class>/`,
//! dispatches the portfolio on each check_* property, records wall-clock
//! per property, and writes 95th-percentile budgets to `corpus/budgets.toml`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use vergil_core::portfolio::{dispatch, PortfolioConfig, Verdict};

#[derive(Parser)]
#[command(name = "vergilbench-calibration")]
struct Args {
    /// Output budgets TOML.
    #[arg(long, default_value = "corpus/budgets.toml")]
    output: PathBuf,

    /// Per-property wall-clock budget for measurement.
    #[arg(long, default_value_t = 300)]
    budget_seconds: u64,

    /// Override calibration dir (default: vergilbench/calibration/contracts).
    #[arg(long)]
    calibration_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let calibration_dir = args.calibration_dir.unwrap_or_else(|| {
        repo_root()
            .join("vergilbench")
            .join("calibration")
            .join("contracts")
    });

    if !calibration_dir.is_dir() {
        return Err(anyhow!(
            "calibration dir not found: {}",
            calibration_dir.display()
        ));
    }

    let budget = Duration::from_secs(args.budget_seconds);
    let mut measurements: BTreeMap<String, Vec<Measurement>> = BTreeMap::new();

    for class in ["trivial", "cheap", "medium", "hard"] {
        let class_dir = calibration_dir.join(class);
        if !class_dir.is_dir() {
            eprintln!("skipping {class}: no directory");
            continue;
        }
        // The class dir is itself a Foundry project (foundry.toml + src/ + test/).
        // Walk any nested sub-projects too so calibrations can grow without
        // changing the runner.
        let projects = if class_dir.join("foundry.toml").is_file() {
            vec![class_dir.clone()]
        } else {
            iter_subdirs(&class_dir)?
                .into_iter()
                .filter(|p| p.join("foundry.toml").is_file())
                .collect()
        };
        for project in projects {
            let project_measurements = run_project(&project, budget).await?;
            for m in project_measurements {
                eprintln!(
                    "[{class}] {} :: {} -> {} in {}ms",
                    project.file_name().unwrap().to_string_lossy(),
                    m.property,
                    m.verdict_kind,
                    m.wall_clock_ms
                );
                measurements.entry(class.to_string()).or_default().push(m);
            }
        }
    }

    let report = build_report(&measurements);
    let out_dir = args
        .output
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&out_dir).with_context(|| format!("create {}", out_dir.display()))?;
    fs::write(&args.output, &report).with_context(|| format!("write {}", args.output.display()))?;
    eprintln!("wrote {}", args.output.display());
    Ok(())
}

fn iter_subdirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            out.push(entry.path());
        }
    }
    out.sort();
    Ok(out)
}

#[derive(Debug, Clone)]
struct Measurement {
    property: String,
    wall_clock_ms: u64,
    verdict_kind: String,
}

async fn run_project(project: &Path, budget: Duration) -> Result<Vec<Measurement>> {
    let properties_path = project.join("test").join("Properties.t.sol");
    if !properties_path.is_file() {
        eprintln!("skipping {}: no test/Properties.t.sol", project.display());
        return Ok(Vec::new());
    }
    let source = fs::read_to_string(&properties_path)
        .with_context(|| format!("read {}", properties_path.display()))?;
    let check_fns = extract_check_functions(&source);
    if check_fns.is_empty() {
        eprintln!("no check_* functions in {}", properties_path.display());
        return Ok(Vec::new());
    }

    let smt_source = first_sol_in_src(project).unwrap_or(properties_path.clone());

    let mut out = Vec::new();
    for property in check_fns {
        let cfg = PortfolioConfig {
            project: project.to_path_buf(),
            property: property.clone(),
            smtchecker_source: smt_source.clone(),
            budget,
        };
        let start = Instant::now();
        let result = dispatch(cfg).await;
        let wall = start.elapsed().as_millis() as u64;
        let kind = match &result.verdict {
            Verdict::Verified { .. } => "verified",
            Verdict::Counterexample { .. } => "counterexample",
            Verdict::Unknown { .. } => "unknown",
            Verdict::Error { .. } => "error",
        };
        out.push(Measurement {
            property,
            wall_clock_ms: wall,
            verdict_kind: kind.to_string(),
        });
    }
    Ok(out)
}

fn extract_check_functions(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("function check_") {
            if let Some(paren) = rest.find('(') {
                out.push(format!("check_{}", &rest[..paren]));
            }
        }
    }
    out
}

fn first_sol_in_src(project: &Path) -> Option<PathBuf> {
    let src = project.join("src");
    let entries = fs::read_dir(&src).ok()?;
    for entry in entries.flatten() {
        if entry
            .path()
            .extension()
            .map(|s| s == "sol")
            .unwrap_or(false)
        {
            return Some(entry.path());
        }
    }
    None
}

/// Build the `budgets.toml` text. For each class, p95 = sorted[ceil(0.95 * n) - 1].
fn build_report(measurements: &BTreeMap<String, Vec<Measurement>>) -> String {
    let mut out = String::new();
    out.push_str("# Vergil empirical solver budgets (Phase 1 calibration).\n");
    out.push_str("# Generated by `vergilbench-calibration`. Edit by re-running, not by hand.\n\n");

    for (class, ms) in measurements {
        let mut sorted: Vec<u64> = ms.iter().map(|m| m.wall_clock_ms).collect();
        sorted.sort_unstable();
        let n = sorted.len();
        let p95 = if n == 0 {
            0
        } else {
            let idx = ((n as f64) * 0.95).ceil() as usize;
            sorted[idx.min(n) - 1]
        };
        let max = sorted.last().copied().unwrap_or(0);
        let min = sorted.first().copied().unwrap_or(0);
        out.push_str(&format!("[{class}]\n"));
        out.push_str(&format!("samples = {n}\n"));
        out.push_str(&format!("min_ms = {min}\n"));
        out.push_str(&format!("p95_ms = {p95}\n"));
        out.push_str(&format!("max_ms = {max}\n"));
        out.push_str("properties = [\n");
        for m in ms {
            out.push_str(&format!(
                "  {{ name = \"{}\", verdict = \"{}\", wall_clock_ms = {} }},\n",
                m.property, m.verdict_kind, m.wall_clock_ms
            ));
        }
        out.push_str("]\n\n");
    }
    out
}

fn repo_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p
}
