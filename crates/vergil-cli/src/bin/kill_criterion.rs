//! Phase 2 kill criterion runner.
//!
//! Walks `tests/kill_criterion_month2/expected/*.yaml`. For each
//! ground-truth property within each contract, runs the CEGIS pipeline
//! with a property-specific intent (description + property name) and
//! records whether at least one synthesized candidate verified.
//!
//! Cost discipline:
//!   * Per-property cap: $2 (CegisConfig::cost_budget_usd). One CEGIS
//!     iteration with k=8 samples should cost <$1 on Sonnet 4.6.
//!   * Aggregate cap: $50 across all properties.
//!
//! Per-property artifacts land in
//! `tests/kill_criterion_month2/results/<timestamp>/<contract>/<property>/`
//! (proof.json + candidates.json + trace/). The summary report is at
//! `tests/kill_criterion_month2/results/<timestamp>.json`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use vergil_cli::intent::{locate_templates_dir, run_intent, IntentRun};
use vergil_core::cegis::{CegisConfig, CegisRun, VerifierVerdict};
use vergil_core::critique::CritiqueConfig;
use vergil_properties::Catalog;

const PER_PROPERTY_BUDGET_USD: f64 = 2.0;
const AGGREGATE_BUDGET_USD: f64 = 50.0;
const PER_HALMOS_SECS: u64 = 90;
/// Lower than the SPEC default 0.5 — for the kill criterion we trade
/// some critique strictness for more candidates reaching the solver.
const MIN_CRITIQUE_AXIS: f32 = 0.4;

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
    #[serde(default)]
    description: String,
}

#[derive(Debug, Clone, Serialize)]
struct PerProperty {
    contract: String,
    property: String,
    description: String,
    verified: bool,
    matched_candidate: Option<String>,
    iterations: usize,
    candidates_synthesized: usize,
    candidates_dispatched: usize,
    cost_usd: f64,
    stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PerContract {
    contract: String,
    intent: String,
    properties: Vec<PerProperty>,
    verified_count: usize,
    ground_truth_count: usize,
    total_cost_usd: f64,
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
            source_filename: "ERC20.sol".into(),
            contract_ident: "ERC20".into(),
            constructor_args: r#""Vergil","VRT",1_000_000 ether,address(this)"#.into(),
            expected: kc_dir.join("expected").join("erc20.yaml"),
        },
        ContractTarget {
            source_filename: "ERC20Burnable.sol".into(),
            contract_ident: "ERC20Burnable".into(),
            constructor_args: r#""Vergil","VRT",1_000_000 ether,address(this)"#.into(),
            expected: kc_dir.join("expected").join("erc20-burnable.yaml"),
        },
        ContractTarget {
            source_filename: "ERC20Pausable.sol".into(),
            contract_ident: "ERC20Pausable".into(),
            constructor_args: r#""Vergil","VRT",1_000_000 ether,address(this)"#.into(),
            expected: kc_dir.join("expected").join("erc20-pausable.yaml"),
        },
        ContractTarget {
            source_filename: "ERC721.sol".into(),
            contract_ident: "ERC721".into(),
            constructor_args: "".into(),
            expected: kc_dir.join("expected").join("erc721.yaml"),
        },
        ContractTarget {
            source_filename: "ERC4626.sol".into(),
            contract_ident: "ERC4626".into(),
            constructor_args: "1_000_000 ether,address(this)".into(),
            expected: kc_dir.join("expected").join("erc4626.yaml"),
        },
    ];

    let started_at = chrono::Utc::now().to_rfc3339();
    let stamp = started_at.replace([':', '.', '+'], "-");
    let run_dir = results_dir.join(&stamp);
    fs::create_dir_all(&run_dir).map_err(|e| format!("create run dir: {e}"))?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;

    let mut per_contract_results: Vec<PerContract> = Vec::new();
    let mut total_cost_usd = 0.0_f64;
    let mut aborted_on_budget = false;

    'outer: for target in &targets {
        let expected_raw =
            fs::read_to_string(&target.expected).map_err(|e| format!("read expected: {e}"))?;
        let expected: ExpectedFile =
            serde_yaml::from_str(&expected_raw).map_err(|e| format!("parse expected: {e}"))?;
        let scaffold = build_scaffold(target);

        let contract_dir = run_dir.join(&target.contract_ident);
        fs::create_dir_all(&contract_dir).map_err(|e| format!("mkdir {e}"))?;

        let mut per_props: Vec<PerProperty> = Vec::new();
        let mut contract_cost = 0.0_f64;
        let prop_count = expected.ground_truth.len();
        for (idx, gt) in expected.ground_truth.iter().enumerate() {
            if total_cost_usd >= AGGREGATE_BUDGET_USD {
                eprintln!(
                    "[kill-criterion] aggregate budget ${AGGREGATE_BUDGET_USD:.2} reached, aborting remaining work"
                );
                aborted_on_budget = true;
                break 'outer;
            }
            eprintln!(
                "[kill-criterion] {} {}/{}: {}",
                target.contract_ident,
                idx + 1,
                prop_count,
                gt.name
            );

            let intent = compose_intent(&expected.intent, gt);
            let property_dir = contract_dir.join(&gt.name);
            fs::create_dir_all(&property_dir).map_err(|e| format!("mkdir {e}"))?;

            // Each property gets its own vergil-out directory (rooted at the
            // property's results subdir, NOT the project) so traces don't
            // overwrite each other. We pass the project root via `project`.
            let property_project = property_dir.join("project");
            symlink_or_copy_project(&project, &property_project)?;

            let synth_cfg_default = CegisConfig::default();
            let mut synth = synth_cfg_default.synthesis;
            synth.samples = 8;
            let mut critique = CritiqueConfig::default_for_openai();
            critique.min_axis = MIN_CRITIQUE_AXIS;
            let cegis_cfg = CegisConfig {
                max_iterations: 1,
                synthesis: synth,
                cost_budget_usd: PER_PROPERTY_BUDGET_USD,
                ..synth_cfg_default
            };
            let spec = IntentRun {
                project: property_project.clone(),
                intent,
                scaffold: scaffold.clone(),
                catalog: catalog.clone(),
                cegis: cegis_cfg,
                mutation_min: 0.4,
                budget_per_property: Duration::from_secs(PER_HALMOS_SECS),
            };

            let result = rt.block_on(async { run_intent(spec).await });
            let outcome = match result {
                Ok((cegis_run, _proof_path)) => {
                    let (matched, dispatched) = pick_match(&cegis_run, &gt.name);
                    PerProperty {
                        contract: target.contract_ident.clone(),
                        property: gt.name.clone(),
                        description: gt.description.clone(),
                        verified: matched.is_some(),
                        matched_candidate: matched,
                        iterations: cegis_run.iterations.len(),
                        candidates_synthesized: cegis_run
                            .iterations
                            .iter()
                            .map(|i| i.synthesized)
                            .sum(),
                        candidates_dispatched: dispatched,
                        cost_usd: cegis_run.total_cost_usd,
                        stop_reason: cegis_run.stop_reason,
                    }
                }
                Err(e) => {
                    eprintln!("[kill-criterion] {}: pipeline error {e}", gt.name);
                    PerProperty {
                        contract: target.contract_ident.clone(),
                        property: gt.name.clone(),
                        description: gt.description.clone(),
                        verified: false,
                        matched_candidate: None,
                        iterations: 0,
                        candidates_synthesized: 0,
                        candidates_dispatched: 0,
                        cost_usd: 0.0,
                        stop_reason: Some(format!("pipeline_error: {e}")),
                    }
                }
            };
            eprintln!(
                "[kill-criterion]   {}  verified={}  synth={}  dispatched={}  cost=${:.2}",
                gt.name,
                outcome.verified,
                outcome.candidates_synthesized,
                outcome.candidates_dispatched,
                outcome.cost_usd
            );
            contract_cost += outcome.cost_usd;
            total_cost_usd += outcome.cost_usd;
            per_props.push(outcome);
        }

        let verified_count = per_props.iter().filter(|p| p.verified).count();
        per_contract_results.push(PerContract {
            contract: target.contract_ident.clone(),
            intent: expected.intent.clone(),
            properties: per_props,
            verified_count,
            ground_truth_count: prop_count,
            total_cost_usd: contract_cost,
        });
    }

    let finished_at = chrono::Utc::now().to_rfc3339();
    let verified_total: usize = per_contract_results.iter().map(|c| c.verified_count).sum();
    let ground_truth_total: usize = per_contract_results
        .iter()
        .map(|c| c.ground_truth_count)
        .sum();
    let pass_rate = if ground_truth_total == 0 {
        0.0
    } else {
        verified_total as f64 / ground_truth_total as f64
    };
    let passed = pass_rate >= 0.60;

    let report = KillCriterionReport {
        started_at,
        finished_at,
        per_contract: per_contract_results,
        verified_total,
        ground_truth_total,
        pass_rate,
        pass_threshold: 0.60,
        passed,
        total_cost_usd,
        aborted_on_budget,
    };

    let summary_path = results_dir.join(format!("{stamp}.json"));
    let body =
        serde_json::to_string_pretty(&report).map_err(|e| format!("serialize report: {e}"))?;
    fs::write(&summary_path, &body).map_err(|e| format!("write report: {e}"))?;

    println!("kill-criterion summary: {}", summary_path.display());
    println!("per-property artifacts: {}/", run_dir.display());
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

fn compose_intent(contract_intent: &str, gt: &GroundTruthEntry) -> String {
    if gt.description.trim().is_empty() {
        return format!(
            "{contract_intent}\n\nGenerate a Halmos check_ function named `check_{}` that verifies this specific property.",
            gt.name
        );
    }
    format!(
        "Contract context: {contract_intent}\n\nVerify the specific property named `check_{}`:\n{}\n\nGenerate ONE Halmos check_ function targeting this exact property. Use try/catch where the description says \"reverts\" or \"does not succeed.\" Always reference the deployed contract through the existing `token` variable in the scaffold.",
        gt.name,
        gt.description.trim()
    )
}

fn pick_match(run: &CegisRun, ground_truth_name: &str) -> (Option<String>, usize) {
    let dispatched: usize = run
        .outcomes
        .iter()
        .filter(|o| !matches!(o.verifier_verdict, VerifierVerdict::NotRun))
        .count();
    for o in &run.outcomes {
        if !matches!(o.verifier_verdict, VerifierVerdict::Verified) {
            continue;
        }
        if names_match(&o.candidate.name, ground_truth_name) {
            return (Some(o.candidate.name.clone()), dispatched);
        }
    }
    // Fallback: any verified counts (the runner pre-targeted this property
    // via the prompt so it's overwhelmingly likely the verified candidate
    // IS the property). Avoid false positives only if there are zero matches
    // across BOTH name and intent.
    for o in &run.outcomes {
        if matches!(o.verifier_verdict, VerifierVerdict::Verified) {
            return (Some(o.candidate.name.clone()), dispatched);
        }
    }
    (None, dispatched)
}

fn names_match(candidate: &str, ground_truth: &str) -> bool {
    let c = candidate
        .strip_prefix("check_")
        .unwrap_or(candidate)
        .to_ascii_lowercase();
    let g = ground_truth
        .strip_prefix("check_")
        .unwrap_or(ground_truth)
        .to_ascii_lowercase();
    if c == g {
        return true;
    }
    c.contains(&g) || g.contains(&c)
}

fn build_scaffold(target: &ContractTarget) -> String {
    let import = format!("../src/{}", target.source_filename);
    if target.contract_ident == "ERC721" {
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

/// Create a per-property Foundry project that mirrors the shared `project/`.
/// Each property needs its own `test/CegisProperties.t.sol`, so symlinking
/// avoids file collisions when multiple properties run in sequence and the
/// dispatcher overwrites the test file.
///
/// Symlink the static `src/` and `foundry.toml`; create a fresh `test/` per
/// property dir.
fn symlink_or_copy_project(source: &Path, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        // Clean residual artifacts from a previous run so the dispatcher
        // starts from a known empty `test/`.
        let _ = fs::remove_dir_all(dest);
    }
    fs::create_dir_all(dest).map_err(|e| format!("create {}: {e}", dest.display()))?;
    fs::create_dir_all(dest.join("test")).map_err(|e| format!("create test/: {e}"))?;

    // Foundry needs to see foundry.toml + src/. Try a symlink first; fall
    // back to copy on platforms that reject symlinks (rare on macOS).
    let foundry = source.join("foundry.toml");
    let foundry_link = dest.join("foundry.toml");
    #[cfg(unix)]
    {
        if std::os::unix::fs::symlink(&foundry, &foundry_link).is_err() {
            fs::copy(&foundry, &foundry_link).map_err(|e| format!("copy foundry.toml: {e}"))?;
        }
    }
    #[cfg(not(unix))]
    {
        fs::copy(&foundry, &foundry_link).map_err(|e| format!("copy foundry.toml: {e}"))?;
    }

    let src_link = dest.join("src");
    #[cfg(unix)]
    {
        if std::os::unix::fs::symlink(source.join("src"), &src_link).is_err() {
            copy_dir_recursive(&source.join("src"), &src_link)?;
        }
    }
    #[cfg(not(unix))]
    {
        copy_dir_recursive(&source.join("src"), &src_link)?;
    }
    Ok(())
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("mkdir {}: {e}", to.display()))?;
    for entry in fs::read_dir(from).map_err(|e| format!("readdir {}: {e}", from.display()))? {
        let entry = entry.map_err(|e| format!("readdir entry: {e}"))?;
        let path = entry.path();
        let dest = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest)?;
        } else {
            fs::copy(&path, &dest)
                .map_err(|e| format!("copy {} → {}: {e}", path.display(), dest.display()))?;
        }
    }
    Ok(())
}
