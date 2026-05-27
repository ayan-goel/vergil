use std::path::{Path, PathBuf};
use std::time::Duration;

use vergil_core::cegis::{CegisConfig, VerifierVerdict};
use vergil_core::portfolio::{dispatch, PortfolioConfig, Verdict};
use vergil_properties::Catalog;
use vergil_solidity::foundry::{emit_counterexample, PropertyContext};
use vergil_solidity::halmos::HalmosResult;

use crate::commands::intent::{
    default_scaffold_for_erc20, locate_templates_dir, run_intent, IntentRun,
};
use crate::config::{self, PropertiesFile};
use crate::output::{markdown, text, PropertyOutcome, VerifyReport};
use crate::OutputFormat;

const DEFAULT_BUDGET_SECS: u64 = 120;
const DEFAULT_PROPERTY_CONTRACT: &str = "Properties";

/// Remove any `test/Cex_*.t.sol` files left over from previous verify runs.
/// They would otherwise be compiled by the next Halmos invocation and break
/// the build if their contents are inconsistent with the current code.
fn cleanup_stale_cex_files(project: &std::path::Path) {
    let test_dir = project.join("test");
    let Ok(entries) = std::fs::read_dir(&test_dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("Cex_") && name.ends_with(".t.sol") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

pub async fn run(
    project: PathBuf,
    properties: Option<PathBuf>,
    format: OutputFormat,
    intent: Option<String>,
) -> Result<(), u8> {
    if let Some(intent_str) = intent {
        return run_with_intent(project, intent_str, format).await;
    }
    let project = match project.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("invalid project path {}: {e}", project.display());
            return Err(3);
        }
    };

    let props_path = properties.unwrap_or_else(|| project.join("properties.yaml"));
    let props = match config::load(&props_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("properties: {e}");
            return Err(3);
        }
    };

    // Sweep any Cex_*.t.sol files emitted by a previous run — they import the
    // property contract and would break compilation if anything else changed.
    cleanup_stale_cex_files(&project);

    let outcomes = run_all_properties(&project, &props).await;
    let report = VerifyReport {
        project: project.display().to_string(),
        properties: outcomes,
    };

    emit_counterexample_files(&project, &props, &report)?;

    match format {
        OutputFormat::Text => {
            print!("{}", text::render(&report));
        }
        OutputFormat::Markdown => {
            let body = markdown::render(&report);
            let out_dir = project.join("vergil-out");
            if let Err(e) = std::fs::create_dir_all(&out_dir) {
                eprintln!("create vergil-out: {e}");
                return Err(3);
            }
            let out_file = out_dir.join("report.md");
            if let Err(e) = std::fs::write(&out_file, &body) {
                eprintln!("write {}: {e}", out_file.display());
                return Err(3);
            }
            println!("wrote {}", out_file.display());
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&report).unwrap_or_default();
            println!("{json}");
        }
    }

    let code = report.exit_code();
    if code == 0 {
        Ok(())
    } else {
        Err(code)
    }
}

async fn run_all_properties(project: &Path, props: &PropertiesFile) -> Vec<PropertyOutcome> {
    let mut outcomes = Vec::with_capacity(props.properties.len());
    let smt_default = default_smt_source(project);

    for entry in &props.properties {
        let smt_source = entry
            .smtchecker_source
            .as_deref()
            .map(|s| project.join(s))
            .unwrap_or_else(|| smt_default.clone());
        let cfg = PortfolioConfig {
            project: project.to_path_buf(),
            property: entry.name.clone(),
            smtchecker_source: smt_source,
            budget: Duration::from_secs(DEFAULT_BUDGET_SECS),
            capture_smt_queries: false,
        };
        let result = dispatch(cfg).await;
        outcomes.push(PropertyOutcome {
            name: entry.name.clone(),
            result,
        });
    }
    outcomes
}

fn default_smt_source(project: &Path) -> PathBuf {
    let src = project.join("src");
    if let Ok(entries) = std::fs::read_dir(&src) {
        for e in entries.flatten() {
            if e.path().extension().map(|s| s == "sol").unwrap_or(false) {
                return e.path();
            }
        }
    }
    src.join("Token.sol")
}

fn emit_counterexample_files(
    project: &Path,
    props: &PropertiesFile,
    report: &VerifyReport,
) -> Result<(), u8> {
    let mut any = false;
    let out_dir = project.join("vergil-out").join("counterexamples");
    for outcome in &report.properties {
        let Verdict::Counterexample { .. } = &outcome.result.verdict else {
            continue;
        };
        let Some(entry) = props.properties.iter().find(|p| p.name == outcome.name) else {
            continue;
        };

        let trace = match halmos_trace_for(project, &outcome.name) {
            Some(t) => t,
            None => continue,
        };

        // `path` in the YAML is the import the EMITTED Cex_*.t.sol uses, so it
        // must be relative to test/ (where the emitted file lives). Use as-is.
        let (contract_name, import_path, ctor_args_owned) = match &props.property_contract {
            Some(pc) => (
                pc.name.clone(),
                pc.path.clone(),
                pc.constructor_args.clone(),
            ),
            None => (
                DEFAULT_PROPERTY_CONTRACT.to_string(),
                "./Properties.t.sol".to_string(),
                Vec::<String>::new(),
            ),
        };

        let params_owned: Vec<(String, String)> = entry
            .params
            .iter()
            .map(|p| (p.name.clone(), p.solidity_type.clone()))
            .collect();
        let params: Vec<(&str, &str)> = params_owned
            .iter()
            .map(|(n, t)| (n.as_str(), t.as_str()))
            .collect();
        let ctor_args: Vec<&str> = ctor_args_owned.iter().map(String::as_str).collect();
        let ctx = PropertyContext {
            contract_name: contract_name.as_str(),
            import_path: import_path.as_str(),
            params: params.as_slice(),
            constructor_args: ctor_args.as_slice(),
        };
        let src = emit_counterexample(&trace, &ctx);

        if !any {
            if let Err(e) = std::fs::create_dir_all(&out_dir) {
                eprintln!("create {}: {e}", out_dir.display());
                return Err(3);
            }
            any = true;
        }
        let file = out_dir.join(format!("Cex_{}.t.sol", outcome.name));
        if let Err(e) = std::fs::write(&file, &src) {
            eprintln!("write {}: {e}", file.display());
            return Err(3);
        }
        let live = project
            .join("test")
            .join(format!("Cex_{}.t.sol", outcome.name));
        if let Err(e) = std::fs::write(&live, &src) {
            eprintln!("write {}: {e}", live.display());
            return Err(3);
        }
    }
    Ok(())
}

/// End-to-end `vergil verify --intent` path: build env providers, run CEGIS,
/// serialize proof.json. Exit codes:
///   0 — at least one property verified, proof.json written
///   1 — CEGIS finished but no property verified (counterexample or unknown)
///   2 — pipeline failure (env, IO, retrieval, etc.)
async fn run_with_intent(project: PathBuf, intent: String, format: OutputFormat) -> Result<(), u8> {
    let project = match project.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("invalid project path {}: {e}", project.display());
            return Err(3);
        }
    };
    let templates_dir = match locate_templates_dir() {
        Some(p) => p,
        None => {
            eprintln!(
                "could not locate property templates dir. Run from the Vergil repo root, or \
                 set CARGO_MANIFEST_DIR to a workspace member."
            );
            return Err(2);
        }
    };
    let catalog = match Catalog::load(&templates_dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("templates {}: {e}", templates_dir.display());
            return Err(2);
        }
    };

    // CLI defaults: tighter than production. Stretching the per-contract
    // budget to $10 caps blast radius from a single interactive run. k=4
    // trims fan-out (the kill-criterion runner uses k=16 for the sweep).
    // 3 iterations is enough for the typical synth → critique → verify
    // flow before refinement diverges.
    let mut synth = CegisConfig::default().synthesis;
    synth.samples = 4;
    let cegis_cfg = CegisConfig {
        max_iterations: 3,
        synthesis: synth,
        cost_budget_usd: 10.0,
        ..CegisConfig::default()
    };

    let spec = IntentRun {
        project: project.clone(),
        intent: intent.clone(),
        description: None,
        scaffold: default_scaffold_for_erc20(),
        catalog,
        cegis: cegis_cfg,
        min_critique_axis: None,
        mutation_min: 0.4,
        budget_per_property: Duration::from_secs(DEFAULT_BUDGET_SECS),
    };

    let (run, proof_path) = match run_intent(spec).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("intent run failed: {e}");
            return Err(2);
        }
    };

    let verified: Vec<&_> = run
        .outcomes
        .iter()
        .filter(|o| matches!(o.verifier_verdict, VerifierVerdict::Verified { .. }))
        .collect();
    match format {
        OutputFormat::Text => {
            println!("intent: {intent}");
            println!("iterations: {}", run.iterations.len());
            println!("synthesized: {} candidates", run.outcomes.len());
            println!("verified: {}", verified.len());
            for v in &verified {
                println!("  ✓ {}", v.candidate.name);
            }
            for o in &run.outcomes {
                if let VerifierVerdict::Counterexample(msg) = &o.verifier_verdict {
                    println!("  ✗ {}: {msg}", o.candidate.name);
                }
            }
            println!(
                "cost: ${:.4} ({}/{} tokens)",
                run.total_cost_usd,
                total_tokens(&run, true),
                total_tokens(&run, false)
            );
            println!("proof: {}", proof_path.display());
            if let Some(reason) = &run.stop_reason {
                println!("stop_reason: {reason}");
            }
        }
        OutputFormat::Markdown => {
            let body = format!(
                "# Vergil intent run\n\n- intent: `{}`\n- iterations: {}\n- verified: {} / {}\n- cost: ${:.4}\n- proof: `{}`\n",
                intent,
                run.iterations.len(),
                verified.len(),
                run.outcomes.len(),
                run.total_cost_usd,
                proof_path.display()
            );
            let out = project.join("vergil-out").join("report.md");
            if let Err(e) = std::fs::write(&out, &body) {
                eprintln!("write {}: {e}", out.display());
                return Err(3);
            }
            println!("wrote {}", out.display());
        }
        OutputFormat::Json => {
            let value = serde_json::json!({
                "intent": intent,
                "iterations": run.iterations.len(),
                "synthesized": run.outcomes.len(),
                "verified": verified.iter().map(|o| &o.candidate.name).collect::<Vec<_>>(),
                "cost_usd": run.total_cost_usd,
                "proof_path": proof_path.display().to_string(),
                "stop_reason": run.stop_reason,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_default()
            );
        }
    }

    if verified.is_empty() {
        Err(1)
    } else {
        Ok(())
    }
}

fn total_tokens(run: &vergil_core::cegis::CegisRun, is_in: bool) -> u32 {
    if is_in {
        run.iterations.iter().map(|i| i.tokens_in).sum()
    } else {
        run.iterations.iter().map(|i| i.tokens_out).sum()
    }
}

/// Re-run Halmos in a thread-isolated tokio runtime to capture a structured
/// trace. Phase 1 chooses simplicity over threading the trace through the
/// portfolio dispatch result; the second Halmos call is a cache hit (<1s).
fn halmos_trace_for(project: &Path, property: &str) -> Option<vergil_solidity::halmos::Trace> {
    let project = project.to_path_buf();
    let property = property.to_string();
    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        rt.block_on(async {
            let res = vergil_solidity::halmos::run_simple(
                &project,
                &property,
                Duration::from_secs(DEFAULT_BUDGET_SECS),
            )
            .await;
            match res {
                HalmosResult::Counterexample { trace, .. } => Some(trace),
                _ => None,
            }
        })
    });
    handle.join().ok().flatten()
}
