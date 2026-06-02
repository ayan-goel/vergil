use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use vergil_core::cegis::{CegisConfig, CegisLoop, VerifierVerdict};
use vergil_core::portfolio::{dispatch, PortfolioConfig, Verdict};
use vergil_core::telemetry::{JsonlSink, TelemetrySink};
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

#[allow(clippy::too_many_arguments)]
pub async fn run(
    project: PathBuf,
    properties: Option<PathBuf>,
    format: OutputFormat,
    intent: Option<String>,
    scaffold_override: Option<PathBuf>,
    telemetry_json: Option<PathBuf>,
    tenant: String,
    cost_budget: Option<f64>,
    samples: Option<usize>,
    min_critique_axis: Option<f32>,
) -> Result<(), u8> {
    if let Some(intent_str) = intent {
        return run_with_intent(
            project,
            intent_str,
            format,
            scaffold_override,
            telemetry_json,
            tenant,
            cost_budget,
            samples,
            min_critique_axis,
        )
        .await;
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

    // Write proof.json so `vergil prove` can re-check the source hashes
    // and (Phase 4) re-dispatch the SMT queries without a fresh CEGIS run.
    let proof_intent = format!("properties.yaml: {}", props_path.display());
    if let Err(e) = emit_phase1_proof(&project, &proof_intent, &report) {
        eprintln!("proof.json: {e}");
        return Err(3);
    }

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
            // Phase 1 path enables SMT capture + persistence so proof.json
            // carries smt_query_sha256 and `vergil prove` can re-dispatch.
            budget: Duration::from_secs(DEFAULT_BUDGET_SECS),
            capture_smt_queries: true,
            smt_persist_dir: Some(project.join("vergil-out").join("smt")),
        };
        let result = dispatch(cfg).await;
        outcomes.push(PropertyOutcome {
            name: entry.name.clone(),
            result,
        });
    }
    outcomes
}

/// Convert a Phase 1 verify run's outcomes into a proof.json artifact and
/// write it under `<project>/vergil-out/proof.json`. Mirrors the schema
/// emitted by the Phase 2 intent flow so `vergil prove` accepts either.
fn emit_phase1_proof(project: &Path, intent: &str, report: &VerifyReport) -> Result<(), String> {
    use vergil_proof::schema::{
        sha256_hex, Cost, CounterexampleSummary, ManifestValidationStatus, ProofArtifact,
        QualityMetrics, RunMeta, SourceFile, ToolchainVersions, VerifiedProperty,
    };

    let mut source_files = Vec::new();
    let src_dir = project.join("src");
    if src_dir.is_dir() {
        let entries = std::fs::read_dir(&src_dir).map_err(|e| format!("read src/: {e}"))?;
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().map(|s| s == "sol").unwrap_or(false) {
                let bytes = std::fs::read(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
                let rel = p
                    .strip_prefix(project)
                    .map(|r| r.display().to_string())
                    .unwrap_or_else(|_| p.display().to_string());
                source_files.push(SourceFile {
                    path: rel,
                    sha256: sha256_hex(&bytes),
                });
            }
        }
    }

    let verified_properties: Vec<VerifiedProperty> = report
        .properties
        .iter()
        .filter_map(|p| match &p.result.verdict {
            Verdict::Verified {
                backend,
                wall_clock_ms,
                smt_query_sha256,
            } => Some(VerifiedProperty {
                name: p.name.clone(),
                backend: backend_to_str(*backend).to_string(),
                spec_sha256: sha256_hex(p.name.as_bytes()),
                template_ref: None,
                wall_clock_ms: *wall_clock_ms,
                smt_query_sha256: smt_query_sha256.clone(),
                manifest_validation: ManifestValidationStatus {
                    storage_ok: true,
                    modifiers_ok: true,
                    external_calls_ok: true,
                    warnings: Vec::new(),
                },
            }),
            _ => None,
        })
        .collect();

    let counterexamples: Vec<CounterexampleSummary> = report
        .properties
        .iter()
        .filter_map(|p| match &p.result.verdict {
            Verdict::Counterexample {
                backend,
                wall_clock_ms,
                message,
                ..
            } => Some(CounterexampleSummary {
                property: p.name.clone(),
                backend: backend_to_str(*backend).to_string(),
                cex_file: format!("counterexamples/Cex_{}.t.sol", p.name),
                wall_clock_ms: *wall_clock_ms,
                trace_summary: message.clone(),
            }),
            _ => None,
        })
        .collect();

    let wall_clock_ms: u64 = report
        .properties
        .iter()
        .map(|p| match &p.result.verdict {
            Verdict::Verified { wall_clock_ms, .. } => *wall_clock_ms,
            Verdict::Counterexample { wall_clock_ms, .. } => *wall_clock_ms,
            _ => 0,
        })
        .sum();

    let proof = ProofArtifact {
        vergil_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: ProofArtifact::schema_version_current(),
        run: RunMeta {
            run_id: chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string(),
            intent: intent.to_string(),
            project_root: project.display().to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
        },
        toolchain: ToolchainVersions {
            solc: "0.8.20".to_string(),
            halmos: "0.3.3".to_string(),
            slither: "0.11.0".to_string(),
            z3: "4.15.4".to_string(),
            cvc5: "1.3.0".to_string(),
            gambit: Some("0.2.1".to_string()),
        },
        source_files,
        verified_properties,
        counterexamples,
        quality_metrics: QualityMetrics {
            // Phase 1 path doesn't run mutation testing or critique — leave the
            // shape valid but zeroed.
            mutation_coverage_min: None,
            critique_pass_rate: 1.0,
            mutation_testing_enabled: false,
        },
        cost: Cost {
            tokens_in: 0,
            tokens_out: 0,
            usd_estimate: 0.0,
            wall_clock_ms,
        },
    };

    let out_dir = project.join("vergil-out");
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("mkdir vergil-out: {e}"))?;
    let out = out_dir.join("proof.json");
    let body =
        serde_json::to_string_pretty(&proof).map_err(|e| format!("serialize proof.json: {e}"))?;
    std::fs::write(&out, body).map_err(|e| format!("write {}: {e}", out.display()))?;
    Ok(())
}

fn backend_to_str(b: vergil_core::portfolio::Backend) -> &'static str {
    match b {
        vergil_core::portfolio::Backend::Halmos => "halmos",
        vergil_core::portfolio::Backend::SmtChecker => "smtchecker",
    }
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
#[allow(clippy::too_many_arguments)]
async fn run_with_intent(
    project: PathBuf,
    intent: String,
    format: OutputFormat,
    scaffold_override: Option<PathBuf>,
    telemetry_json: Option<PathBuf>,
    tenant: String,
    cost_budget: Option<f64>,
    samples: Option<usize>,
    min_critique_axis: Option<f32>,
) -> Result<(), u8> {
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
    synth.samples = samples.unwrap_or(4);
    let cegis_cfg = CegisConfig {
        max_iterations: 3,
        synthesis: synth,
        cost_budget_usd: cost_budget.unwrap_or(10.0),
        tenant_id: tenant.clone(),
        run_id: None,
        ..CegisConfig::default()
    };

    let scaffold = match resolve_scaffold(&project, scaffold_override.as_deref()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("scaffold: {e}");
            return Err(3);
        }
    };

    // Phase 4 Slice B2: open the telemetry sink if --telemetry-json was
    // passed. Failure to open the sink is fatal — opting in means the
    // caller depends on the stream.
    let telemetry: Arc<dyn TelemetrySink> = match telemetry_json.as_deref() {
        Some(p) => match JsonlSink::open(p) {
            Ok(s) => Arc::new(s),
            Err(e) => {
                eprintln!("telemetry json sink {}: {e}", p.display());
                return Err(3);
            }
        },
        None => CegisLoop::null_sink(),
    };

    let spec = IntentRun {
        project: project.clone(),
        intent: intent.clone(),
        description: None,
        scaffold,
        catalog,
        cegis: cegis_cfg,
        min_critique_axis,
        mutation_min: 0.4,
        budget_per_property: Duration::from_secs(DEFAULT_BUDGET_SECS),
        telemetry,
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
    let quality = compute_quality_stats(&run);
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
                if let VerifierVerdict::Counterexample { message } = &o.verifier_verdict {
                    println!("  ✗ {}: {message}", o.candidate.name);
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
            println!();
            println!("quality:");
            println!(
                "  critique pass-rate: {:.0}% ({} accepted / {} synthesized)",
                quality.critique_pass_rate * 100.0,
                quality.critique_accepted,
                quality.critique_total,
            );
            println!(
                "  vacuity score (min/avg/max): {}",
                quality.format_vacuity()
            );
            println!("  mutation coverage: {}", quality.format_mutation());
            println!("  spec-doc consistency: {}", quality.format_spec_doc());
        }
        OutputFormat::Markdown => {
            let mut body = format!(
                "# Vergil intent run\n\n- intent: `{}`\n- iterations: {}\n- verified: {} / {}\n- cost: ${:.4}\n- proof: `{}`\n",
                intent,
                run.iterations.len(),
                verified.len(),
                run.outcomes.len(),
                run.total_cost_usd,
                proof_path.display()
            );
            body.push_str("\n## Quality\n\n");
            body.push_str(&format!(
                "- **Critique pass-rate:** {:.0}% ({} accepted / {} synthesized)\n",
                quality.critique_pass_rate * 100.0,
                quality.critique_accepted,
                quality.critique_total,
            ));
            body.push_str(&format!(
                "- **Vacuity score (min / avg / max):** {}\n",
                quality.format_vacuity()
            ));
            body.push_str(&format!(
                "- **Mutation coverage:** {}\n",
                quality.format_mutation()
            ));
            body.push_str(&format!(
                "- **Spec-doc consistency:** {}\n",
                quality.format_spec_doc()
            ));
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
                "quality": {
                    "critique_pass_rate": quality.critique_pass_rate,
                    "critique_accepted": quality.critique_accepted,
                    "critique_total": quality.critique_total,
                    "vacuity_min": quality.vacuity_min,
                    "vacuity_avg": quality.vacuity_avg,
                    "vacuity_max": quality.vacuity_max,
                    "mutation_coverage_avg": quality.mutation_coverage_avg,
                    "mutation_testing_enabled": quality.mutation_testing_enabled,
                    "spec_doc_consistency": quality.spec_doc_consistency,
                }
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

/// Per-run quality metrics surfaced in `report.md` and `--format json`.
///
/// SPEC §11.3 step 9 lists three quality metrics that must appear in the
/// report. This struct computes them off the live `CegisRun` so the
/// renderer doesn't need to know about the loop internals.
#[derive(Debug, Clone, PartialEq)]
pub struct QualityStats {
    pub critique_total: usize,
    pub critique_accepted: usize,
    pub critique_pass_rate: f32,
    pub vacuity_min: Option<f32>,
    pub vacuity_avg: Option<f32>,
    pub vacuity_max: Option<f32>,
    pub mutation_coverage_avg: Option<f64>,
    pub mutation_testing_enabled: bool,
    /// LLM-graded NatSpec ↔ verified-spec consistency. Off by default —
    /// surfaces as `None`. Phase 4 wires the actual grading call.
    pub spec_doc_consistency: Option<f32>,
}

impl QualityStats {
    pub fn format_vacuity(&self) -> String {
        match (self.vacuity_min, self.vacuity_avg, self.vacuity_max) {
            (Some(lo), Some(avg), Some(hi)) => format!("{lo:.2} / {avg:.2} / {hi:.2}"),
            _ => "no critique data (degraded mode)".to_string(),
        }
    }
    pub fn format_mutation(&self) -> String {
        if !self.mutation_testing_enabled {
            return "disabled (CLI default — opt in with --mutation-test in Phase 4)".to_string();
        }
        match self.mutation_coverage_avg {
            Some(v) => format!("{:.0}% (avg across verified)", v * 100.0),
            None => "enabled but no coverage recorded".to_string(),
        }
    }
    pub fn format_spec_doc(&self) -> String {
        match self.spec_doc_consistency {
            Some(v) => format!("{:.0}%", v * 100.0),
            None => "disabled (LLM-graded check is opt-in, ~$0.02/property)".to_string(),
        }
    }
}

pub fn compute_quality_stats(run: &vergil_core::cegis::CegisRun) -> QualityStats {
    use vergil_core::critique::Verdict as CritiqueVerdict;
    let critiques: Vec<&vergil_core::critique::CritiqueResult> = run
        .outcomes
        .iter()
        .filter_map(|o| o.critique.as_ref())
        .collect();
    let critique_total = critiques.len();
    let critique_accepted = critiques
        .iter()
        .filter(|c| matches!(c.verdict, CritiqueVerdict::Accept))
        .count();
    let critique_pass_rate = if critique_total == 0 {
        0.0
    } else {
        critique_accepted as f32 / critique_total as f32
    };
    let vacuity_vals: Vec<f32> = critiques.iter().map(|c| c.scores.vacuity).collect();
    let (vacuity_min, vacuity_avg, vacuity_max) = if vacuity_vals.is_empty() {
        (None, None, None)
    } else {
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        let mut sum = 0.0;
        for v in &vacuity_vals {
            if *v < lo {
                lo = *v;
            }
            if *v > hi {
                hi = *v;
            }
            sum += v;
        }
        let avg = sum / vacuity_vals.len() as f32;
        (Some(lo), Some(avg), Some(hi))
    };
    let mutation_vals: Vec<f64> = run
        .outcomes
        .iter()
        .filter_map(|o| o.mutation_coverage)
        .collect();
    let mutation_testing_enabled = !mutation_vals.is_empty();
    let mutation_coverage_avg = if mutation_vals.is_empty() {
        None
    } else {
        Some(mutation_vals.iter().sum::<f64>() / mutation_vals.len() as f64)
    };
    QualityStats {
        critique_total,
        critique_accepted,
        critique_pass_rate,
        vacuity_min,
        vacuity_avg,
        vacuity_max,
        mutation_coverage_avg,
        mutation_testing_enabled,
        spec_doc_consistency: None,
    }
}

/// Pick the scaffold for an intent run.
///
/// Resolution order:
///   1. If `--scaffold <path>` was passed, read that file verbatim. It must
///      contain `{{CHECK_FN}}`; `{{NAME}}` is optional.
///   2. Otherwise auto-detect: read the first `.sol` under `<project>/src/`,
///      extract the first top-level `contract <Name>` identifier, and
///      synthesize a default scaffold that imports it and calls `new <Name>()`
///      with empty constructor args. Works for the common case (e.g. ERC-721);
///      contracts with required ctor args need an explicit `--scaffold`.
///   3. If both fail, fall back to the historical `default_scaffold_for_erc20`
///      (preserves examples/erc20 backwards compatibility).
fn resolve_scaffold(project: &Path, override_path: Option<&Path>) -> Result<String, String> {
    if let Some(p) = override_path {
        let body = std::fs::read_to_string(p)
            .map_err(|e| format!("could not read scaffold {}: {e}", p.display()))?;
        if !body.contains("{{CHECK_FN}}") {
            return Err(format!(
                "scaffold {} must contain `{{{{CHECK_FN}}}}` placeholder",
                p.display()
            ));
        }
        return Ok(body);
    }
    match autodetect_scaffold(project) {
        Some(s) => Ok(s),
        None => Ok(default_scaffold_for_erc20()),
    }
}

/// Foundry-style assertion helpers injected into the auto-generated scaffold.
/// Synthesized candidates routinely call `assertEq` / `assertTrue` / etc. (the
/// forge-std `Test` vocabulary), but the bare scaffold does not inherit
/// forge-std, so without these the candidate fails to compile and the verifier
/// reports a build error rather than a verdict (Phase 4 Slice A9). Each helper
/// reduces to a plain `assert`, which Halmos treats as the property to prove.
const ASSERT_HELPERS: &str = r#"    // Foundry-style assertion helpers (synthesized candidates expect these).
    function assertEq(uint256 a, uint256 b) internal pure { assert(a == b); }
    function assertEq(address a, address b) internal pure { assert(a == b); }
    function assertEq(bool a, bool b) internal pure { assert(a == b); }
    function assertEq(bytes32 a, bytes32 b) internal pure { assert(a == b); }
    function assertTrue(bool c) internal pure { assert(c); }
    function assertFalse(bool c) internal pure { assert(!c); }
    function assertGt(uint256 a, uint256 b) internal pure { assert(a > b); }
    function assertGe(uint256 a, uint256 b) internal pure { assert(a >= b); }
    function assertLt(uint256 a, uint256 b) internal pure { assert(a < b); }
    function assertLe(uint256 a, uint256 b) internal pure { assert(a <= b); }"#;

/// Read every `.sol` under `<project>/src/`, collect each contract
/// identifier, and synthesize a scaffold importing all of them.
///
/// Single-contract case (one .sol with one contract): keeps the legacy
/// shape — instantiates the contract as `token` with no constructor args
/// so check_ functions can use it directly.
///
/// Multi-contract case (Phase 4 Slice A4): emits imports for every
/// contract but does NOT pre-instantiate (constructor signatures vary
/// across contracts; let the LLM-authored check_ functions create
/// instances locally with the right arg shape).
///
/// Returns `None` if no .sol files or no contract identifiers found.
fn autodetect_scaffold(project: &Path) -> Option<String> {
    let src_dir = project.join("src");
    let mut sol_files = Vec::new();
    for entry in std::fs::read_dir(&src_dir).ok()?.flatten() {
        let p = entry.path();
        if p.extension().map(|x| x == "sol").unwrap_or(false) {
            sol_files.push(p);
        }
    }
    if sol_files.is_empty() {
        return None;
    }
    // Sort for deterministic scaffold output across runs.
    sol_files.sort();

    // Collect every contract identifier across every file.
    let mut imports: Vec<(String, String)> = Vec::new(); // (ident, filename)
    for path in &sol_files {
        let filename = path.file_name()?.to_string_lossy().into_owned();
        let body = std::fs::read_to_string(path).ok()?;
        for ident in extract_all_contract_names(&body) {
            imports.push((ident, filename.clone()));
        }
    }
    if imports.is_empty() {
        return None;
    }

    if imports.len() == 1 {
        // Single-contract shape: deploy the contract as `token` so candidates
        // can use it directly. Phase 4 Slice A9: parse the constructor and
        // synthesize valid deploy args (the legacy no-arg `new X()` broke every
        // contract with required ctor args — ERC20Capped(cap), Ownable(owner),
        // etc.). Falls back to no-arg when a param type can't be synthesized
        // (interface handles, arrays).
        let (ident, filename) = &imports[0];
        let body = std::fs::read_to_string(src_dir.join(filename)).unwrap_or_default();
        let params = vergil_solidity::signatures::extract_constructor(&body);
        let deploy = match vergil_solidity::signatures::synthesize_ctor_args(&params) {
            Some(args) => format!("new {ident}({args})"),
            None => format!("new {ident}()"),
        };
        return Some(format!(
            r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {{{ident}}} from "../src/{filename}";

contract CegisProperties {{
    {ident} internal token;

    constructor() {{
        token = {deploy};
    }}

{ASSERT_HELPERS}
    {{{{CHECK_FN}}}}
}}
"#
        ));
    }

    // Multi-contract: emit every import, leave constructor empty.
    // check_ functions instantiate locally with the appropriate args.
    let import_lines: String = imports
        .iter()
        .map(|(ident, file)| format!("import {{{ident}}} from \"../src/{file}\";\n"))
        .collect();
    Some(format!(
        r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

{import_lines}
// Phase 4 Slice A4: multi-contract scaffold. Constructor signatures
// vary across imported contracts so we leave instance creation to
// each check_ function. Example pattern:
//   function check_foo() external {{
//       Token t = new Token(1_000_000 ether);
//       Vault v = new Vault(t);
//       assert(v.totalAssets() == t.balanceOf(address(v)));
//   }}
contract CegisProperties {{
{ASSERT_HELPERS}
    {{{{CHECK_FN}}}}
}}
"#
    ))
}

/// Extract every `contract <Name>` (non-`abstract`, non-`library`,
/// non-`interface`) identifier from Solidity source. Returns them in
/// source order.
fn extract_all_contract_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        // Skip `abstract contract`, `library`, `interface` — only
        // concrete `contract <Name>` declarations.
        if trimmed.starts_with("abstract")
            || trimmed.starts_with("library")
            || trimmed.starts_with("interface")
        {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("contract ") {
            let end = rest
                .find(|c: char| c.is_whitespace() || c == '{')
                .unwrap_or(rest.len());
            let name = rest[..end].trim();
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// Extract the first `contract <Name>` (non-`abstract`, non-`library`)
/// identifier from Solidity source. Tolerates inheritance clauses and
/// arbitrary whitespace; matches by line tokenization rather than a full
/// parser since we only need the name.
///
/// Phase 4 Slice A4 retains this single-name helper as a thin wrapper
/// over [`extract_all_contract_names`] so the existing unit tests +
/// any downstream callers that only need the first identifier keep
/// working.
#[cfg(test)]
fn extract_contract_name(source: &str) -> Option<String> {
    extract_all_contract_names(source).into_iter().next()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_contract_name_handles_plain_declaration() {
        let src = "// SPDX\npragma solidity ^0.8.0;\ncontract Foo {\n}\n";
        assert_eq!(extract_contract_name(src), Some("Foo".to_string()));
    }

    #[test]
    fn extract_contract_name_handles_inheritance() {
        let src = "contract MyToken is ERC20, Ownable {\n}";
        assert_eq!(extract_contract_name(src), Some("MyToken".to_string()));
    }

    #[test]
    fn extract_contract_name_skips_leading_whitespace() {
        let src = "    contract Indented {\n}";
        assert_eq!(extract_contract_name(src), Some("Indented".to_string()));
    }

    #[test]
    fn extract_contract_name_returns_none_when_absent() {
        let src = "// no contract keyword here\nlibrary L {}\n";
        assert_eq!(extract_contract_name(src), None);
    }

    #[test]
    fn extract_all_contract_names_handles_multi_contract_file() {
        let src =
            "contract A {}\nlibrary L {}\ninterface I {}\nabstract contract Abs {}\ncontract B {}";
        let names = extract_all_contract_names(src);
        assert_eq!(names, vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn autodetect_scaffold_keeps_single_contract_legacy_shape() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("Solo.sol"),
            "pragma solidity ^0.8.20;\ncontract Solo {}\n",
        )
        .unwrap();
        let s = autodetect_scaffold(tmp.path()).expect("Some");
        assert!(s.contains("Solo internal token"));
        assert!(s.contains("token = new Solo()"));
    }

    #[test]
    fn autodetect_scaffold_handles_multi_contract_src() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("Token.sol"),
            "pragma solidity ^0.8.20;\ncontract Token {}\n",
        )
        .unwrap();
        std::fs::write(
            src.join("Vault.sol"),
            "pragma solidity ^0.8.20;\ncontract Vault {}\n",
        )
        .unwrap();
        let s = autodetect_scaffold(tmp.path()).expect("Some");
        // Both imports present; no pre-instantiation (multi-contract path).
        assert!(s.contains("import {Token}"));
        assert!(s.contains("import {Vault}"));
        assert!(!s.contains("internal token"));
        assert!(s.contains("{{CHECK_FN}}"));
    }

    #[test]
    fn autodetect_scaffold_synthesizes_template_for_known_contract() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("Bar.sol"),
            "pragma solidity ^0.8.20;\ncontract Bar { uint256 public x; }\n",
        )
        .unwrap();
        let s = autodetect_scaffold(tmp.path()).expect("expected Some");
        assert!(s.contains("import {Bar} from \"../src/Bar.sol\""));
        assert!(s.contains("Bar internal token"));
        assert!(s.contains("token = new Bar()"));
        assert!(s.contains("{{CHECK_FN}}"));
    }

    #[test]
    fn resolve_scaffold_respects_explicit_override_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("custom.sol");
        std::fs::write(
            &path,
            "pragma solidity ^0.8.20;\ncontract X { {{CHECK_FN}} }\n",
        )
        .unwrap();
        let s = resolve_scaffold(tmp.path(), Some(&path)).expect("ok");
        assert!(s.contains("contract X"));
        assert!(s.contains("{{CHECK_FN}}"));
    }

    #[test]
    fn resolve_scaffold_rejects_override_missing_check_fn_placeholder() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bad.sol");
        std::fs::write(&path, "no placeholder here").unwrap();
        let err = resolve_scaffold(tmp.path(), Some(&path)).unwrap_err();
        assert!(err.contains("{{CHECK_FN}}"));
    }

    fn fixture_run_with_critiques(pairs: &[(bool, f32)]) -> vergil_core::cegis::CegisRun {
        use vergil_core::cegis::{CandidateOutcome, CegisRun, VerifierVerdict};
        use vergil_core::critique::{CritiqueResult, CritiqueScores, Verdict};
        use vergil_core::synthesis::SpecCandidate;

        let mut outcomes = Vec::new();
        for (accept, vac) in pairs {
            outcomes.push(CandidateOutcome {
                candidate: SpecCandidate {
                    name: "check_x".to_string(),
                    halmos: "function check_x() public {}".to_string(),
                    smtchecker: String::new(),
                    template_ref: None,
                    intent_satisfied: true,
                    source: vergil_core::synthesis::Source::UserIntent,
                    intent_text: None,
                },
                critique: Some(CritiqueResult {
                    verdict: if *accept {
                        Verdict::Accept
                    } else {
                        Verdict::Reject
                    },
                    scores: CritiqueScores {
                        vacuity: *vac,
                        body_independence: 0.5,
                        testability: 0.5,
                        restate_the_source: 1.0,
                    },
                    rationale: String::new(),
                }),
                mutation_coverage: None,
                manifest_warnings: Vec::new(),
                verifier_verdict: VerifierVerdict::NotRun,
            });
        }
        CegisRun {
            outcomes,
            ..Default::default()
        }
    }

    #[test]
    fn compute_quality_stats_reflects_critique_pass_rate() {
        let run = fixture_run_with_critiques(&[(true, 0.8), (true, 0.6), (false, 0.3)]);
        let q = compute_quality_stats(&run);
        assert_eq!(q.critique_total, 3);
        assert_eq!(q.critique_accepted, 2);
        assert!((q.critique_pass_rate - (2.0 / 3.0)).abs() < 1e-6);
        // vacuity min = 0.3, max = 0.8, avg = (0.8 + 0.6 + 0.3) / 3 = 0.566...
        assert!((q.vacuity_min.unwrap() - 0.3).abs() < 1e-6);
        assert!((q.vacuity_max.unwrap() - 0.8).abs() < 1e-6);
        assert!((q.vacuity_avg.unwrap() - 0.566_666_7).abs() < 1e-5);
        // No mutation_coverage on any outcome → mutation testing disabled.
        assert!(!q.mutation_testing_enabled);
        assert!(q.mutation_coverage_avg.is_none());
        assert!(q.spec_doc_consistency.is_none());
    }

    #[test]
    fn compute_quality_stats_handles_empty_run() {
        let run = vergil_core::cegis::CegisRun::default();
        let q = compute_quality_stats(&run);
        assert_eq!(q.critique_total, 0);
        assert_eq!(q.critique_pass_rate, 0.0);
        assert!(q.vacuity_min.is_none());
        assert!(q.format_vacuity().contains("degraded mode"));
        assert!(q.format_mutation().contains("disabled"));
        assert!(q.format_spec_doc().contains("opt-in"));
    }
}
