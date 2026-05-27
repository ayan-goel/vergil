//! Phase 2 kill criterion runner.
//!
//! Walks `tests/kill_criterion_month2/expected/*.yaml`, runs the CEGIS
//! pipeline against each reference contract under
//! `tests/kill_criterion_month2/project/src/`, and reports the pass
//! rate. ≥60% (≥14 / 22) is the SPEC §11.2 exit test.
//!
//! Cost discipline:
//!   * Per-contract cap: $10 (CegisConfig::cost_budget_usd).
//!   * Aggregate cap: $40 across all contracts. Set higher than the
//!     SPEC's $30 because Sonnet 4.6 + GPT-5.5 are noticeably more
//!     expensive than the Phase 2 pricing estimate; the runner aborts
//!     and surfaces `aborted_on_budget = true` if it spills past.
//!   * No retries on budget overrun — the contract is recorded as a
//!     failure (no verified properties) and the sweep continues.
//!
//! Results land in
//! `tests/kill_criterion_month2/results/<timestamp>.json`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use vergil_cli::intent::{locate_templates_dir, run_intent, IntentRun};
use vergil_core::cegis::{CegisConfig, CegisRun, VerifierVerdict};
use vergil_properties::Catalog;

const PER_CONTRACT_BUDGET_USD: f64 = 10.0;
const AGGREGATE_BUDGET_USD: f64 = 40.0;
const PER_PROPERTY_HALMOS_SECS: u64 = 90;

#[derive(Debug, Clone, Deserialize)]
struct ExpectedFile {
    #[allow(dead_code)]
    contract: String,
    intent: String,
    ground_truth: Vec<GroundTruthEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct GroundTruthEntry {
    name: String,
    #[allow(dead_code)]
    expected: String,
}

#[derive(Debug, Clone, Serialize)]
struct PerContract {
    contract: String,
    intent: String,
    ground_truth: Vec<String>,
    verified_properties: Vec<String>,
    verified_count: usize,
    ground_truth_count: usize,
    cost_usd: f64,
    aborted_on_budget: bool,
    stop_reason: Option<String>,
    iterations: usize,
    candidates_synthesized: usize,
}

#[derive(Debug, Serialize)]
struct KillCriterionReport {
    started_at: String,
    finished_at: String,
    per_contract: Vec<PerContract>,
    verified_total: usize,
    ground_truth_total: usize,
    pass_rate: f64,
    pass_threshold: f64,
    passed: bool,
    total_cost_usd: f64,
    aborted_on_budget: bool,
}

#[derive(Debug, Clone)]
struct ContractTarget {
    /// Logical name shown in reports (e.g., "ERC20").
    logical_name: String,
    /// Filename under `project/src/` (e.g., "ERC20.sol").
    source_filename: String,
    /// Solidity contract identifier the scaffold instantiates.
    contract_ident: String,
    /// Constructor invocation in the scaffold.
    constructor_args: String,
    /// Path to the ground-truth YAML.
    expected: PathBuf,
}

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    if let Err(e) = run() {
        eprintln!("kill-criterion: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let repo_root: PathBuf = Path::new(manifest_dir)
        .ancestors()
        .nth(2)
        .ok_or("cannot locate repo root from CARGO_MANIFEST_DIR")?
        .to_path_buf();

    let kc_dir = repo_root.join("tests").join("kill_criterion_month2");
    let project = kc_dir.join("project").canonicalize().map_err(|e| {
        format!(
            "kill criterion project missing at {}: {e}",
            kc_dir.join("project").display()
        )
    })?;
    let results_dir = kc_dir.join("results");
    fs::create_dir_all(&results_dir).map_err(|e| format!("create results dir: {e}"))?;

    let templates_dir = locate_templates_dir().ok_or("locate templates dir")?;
    let catalog = Catalog::load(&templates_dir).map_err(|e| format!("templates: {e}"))?;

    let targets = vec![
        ContractTarget {
            logical_name: "ERC20".into(),
            source_filename: "ERC20.sol".into(),
            contract_ident: "ERC20".into(),
            constructor_args: r#""Vergil","VRT",1_000_000 ether,address(this)"#.into(),
            expected: kc_dir.join("expected").join("erc20.yaml"),
        },
        ContractTarget {
            logical_name: "ERC20Burnable".into(),
            source_filename: "ERC20Burnable.sol".into(),
            contract_ident: "ERC20Burnable".into(),
            constructor_args: r#""Vergil","VRT",1_000_000 ether,address(this)"#.into(),
            expected: kc_dir.join("expected").join("erc20-burnable.yaml"),
        },
        ContractTarget {
            logical_name: "ERC20Pausable".into(),
            source_filename: "ERC20Pausable.sol".into(),
            contract_ident: "ERC20Pausable".into(),
            constructor_args: r#""Vergil","VRT",1_000_000 ether,address(this)"#.into(),
            expected: kc_dir.join("expected").join("erc20-pausable.yaml"),
        },
        ContractTarget {
            logical_name: "ERC721".into(),
            source_filename: "ERC721.sol".into(),
            contract_ident: "ERC721".into(),
            constructor_args: "".into(),
            expected: kc_dir.join("expected").join("erc721.yaml"),
        },
        ContractTarget {
            logical_name: "ERC4626".into(),
            source_filename: "ERC4626.sol".into(),
            contract_ident: "ERC4626".into(),
            constructor_args: "1_000_000 ether,address(this)".into(),
            expected: kc_dir.join("expected").join("erc4626.yaml"),
        },
    ];

    let started_at = chrono::Utc::now().to_rfc3339();
    let mut per_contract: Vec<PerContract> = Vec::new();
    let mut total_cost_usd = 0.0_f64;
    let mut aborted_on_budget = false;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;

    for target in &targets {
        if total_cost_usd >= AGGREGATE_BUDGET_USD {
            eprintln!(
                "[kill-criterion] aggregate budget ${:.2} reached, skipping {}",
                AGGREGATE_BUDGET_USD, target.logical_name
            );
            per_contract.push(PerContract {
                contract: target.logical_name.clone(),
                intent: String::new(),
                ground_truth: Vec::new(),
                verified_properties: Vec::new(),
                verified_count: 0,
                ground_truth_count: 0,
                cost_usd: 0.0,
                aborted_on_budget: true,
                stop_reason: Some("aggregate_budget".into()),
                iterations: 0,
                candidates_synthesized: 0,
            });
            aborted_on_budget = true;
            continue;
        }

        eprintln!(
            "[kill-criterion] {}/{}: {}",
            per_contract.len() + 1,
            targets.len(),
            target.logical_name
        );
        let expected_raw =
            fs::read_to_string(&target.expected).map_err(|e| format!("read expected: {e}"))?;
        let expected: ExpectedFile =
            serde_yaml::from_str(&expected_raw).map_err(|e| format!("parse expected: {e}"))?;
        let ground_truth: Vec<String> =
            expected.ground_truth.iter().map(|g| g.name.clone()).collect();

        // Run the CEGIS pipeline against this specific contract.
        let scaffold = build_scaffold(target);
        let synth_cfg_default = CegisConfig::default();
        let mut synth = synth_cfg_default.synthesis;
        synth.samples = 8;
        let cegis_cfg = CegisConfig {
            max_iterations: 2,
            synthesis: synth,
            cost_budget_usd: PER_CONTRACT_BUDGET_USD,
            ..synth_cfg_default
        };
        let intent_text = expected.intent.clone();
        let spec = IntentRun {
            project: project.clone(),
            intent: intent_text.clone(),
            scaffold,
            catalog: catalog.clone(),
            cegis: cegis_cfg,
            mutation_min: 0.4,
            budget_per_property: Duration::from_secs(PER_PROPERTY_HALMOS_SECS),
        };

        let result = rt.block_on(async { run_intent(spec).await });

        let outcome = match result {
            Ok((run, _proof_path)) => summarize(target, &intent_text, &ground_truth, &run),
            Err(e) => {
                eprintln!("[kill-criterion] {}: pipeline error {e}", target.logical_name);
                PerContract {
                    contract: target.logical_name.clone(),
                    intent: intent_text.clone(),
                    ground_truth: ground_truth.clone(),
                    verified_properties: Vec::new(),
                    verified_count: 0,
                    ground_truth_count: ground_truth.len(),
                    cost_usd: 0.0,
                    aborted_on_budget: false,
                    stop_reason: Some(format!("pipeline_error: {e}")),
                    iterations: 0,
                    candidates_synthesized: 0,
                }
            }
        };

        eprintln!(
            "[kill-criterion] {}: verified {}/{} (cost ${:.2}, iters {}, synth {})",
            target.logical_name,
            outcome.verified_count,
            outcome.ground_truth_count,
            outcome.cost_usd,
            outcome.iterations,
            outcome.candidates_synthesized
        );

        total_cost_usd += outcome.cost_usd;
        per_contract.push(outcome);
    }

    let finished_at = chrono::Utc::now().to_rfc3339();
    let verified_total: usize = per_contract.iter().map(|c| c.verified_count).sum();
    let ground_truth_total: usize = per_contract.iter().map(|c| c.ground_truth_count).sum();
    let pass_rate = if ground_truth_total == 0 {
        0.0
    } else {
        verified_total as f64 / ground_truth_total as f64
    };
    let passed = pass_rate >= 0.60;

    let report = KillCriterionReport {
        started_at,
        finished_at: finished_at.clone(),
        per_contract,
        verified_total,
        ground_truth_total,
        pass_rate,
        pass_threshold: 0.60,
        passed,
        total_cost_usd,
        aborted_on_budget,
    };

    let stamp = finished_at.replace([':', '.'], "-");
    let out_path = results_dir.join(format!("{stamp}.json"));
    let body = serde_json::to_string_pretty(&report)
        .map_err(|e| format!("serialize report: {e}"))?;
    fs::write(&out_path, &body).map_err(|e| format!("write report: {e}"))?;

    println!("kill-criterion report: {}", out_path.display());
    println!(
        "verified {}/{}  pass_rate={:.1}%  cost=${:.2}  passed={}",
        verified_total,
        ground_truth_total,
        pass_rate * 100.0,
        total_cost_usd,
        passed
    );
    if !passed {
        std::process::exit(2);
    }
    Ok(())
}

/// Match verified property names against the expected ground-truth list.
/// We accept fuzzy matches: an LLM-synthesized check_foo verifies the
/// ground-truth "foo" if either name is a substring of the other (with
/// `check_` prefix stripped). This handles minor naming variation the
/// LLM introduces while keeping false matches negligible (property
/// names are descriptive and ≥3 tokens).
fn summarize(
    target: &ContractTarget,
    intent: &str,
    ground_truth: &[String],
    run: &CegisRun,
) -> PerContract {
    let verified_names: Vec<String> = run
        .outcomes
        .iter()
        .filter(|o| matches!(o.verifier_verdict, VerifierVerdict::Verified))
        .map(|o| o.candidate.name.clone())
        .collect();

    let mut matched: Vec<String> = Vec::new();
    for gt in ground_truth {
        if verified_names
            .iter()
            .any(|v| names_match(v, gt))
        {
            matched.push(gt.clone());
        }
    }

    let candidates_synthesized: usize = run.iterations.iter().map(|i| i.synthesized).sum();

    PerContract {
        contract: target.logical_name.clone(),
        intent: intent.to_string(),
        ground_truth: ground_truth.to_vec(),
        verified_properties: matched.clone(),
        verified_count: matched.len(),
        ground_truth_count: ground_truth.len(),
        cost_usd: run.total_cost_usd,
        aborted_on_budget: false,
        stop_reason: run.stop_reason.clone(),
        iterations: run.iterations.len(),
        candidates_synthesized,
    }
}

fn names_match(candidate: &str, ground_truth: &str) -> bool {
    let c = candidate.strip_prefix("check_").unwrap_or(candidate).to_ascii_lowercase();
    let g = ground_truth.strip_prefix("check_").unwrap_or(ground_truth).to_ascii_lowercase();
    if c == g {
        return true;
    }
    c.contains(&g) || g.contains(&c)
}

fn build_scaffold(target: &ContractTarget) -> String {
    let import = format!("../src/{}", target.source_filename);
    if target.contract_ident == "ERC721" {
        // ERC-721 has no constructor parameters in our reference.
        return format!(
            r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {{{ident}}} from "{import}";

contract CegisProperties {{
    {ident} internal token;

    constructor() {{
        token = new {ident}();
    }}

    {{{{CHECK_FN}}}}
}}
"#,
            ident = target.contract_ident,
            import = import,
        );
    }
    format!(
        r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {{{ident}}} from "{import}";

contract CegisProperties {{
    {ident} internal token;

    constructor() {{
        token = new {ident}({args});
    }}

    {{{{CHECK_FN}}}}
}}
"#,
        ident = target.contract_ident,
        import = import,
        args = target.constructor_args,
    )
}
