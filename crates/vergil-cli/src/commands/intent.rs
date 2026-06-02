//! End-to-end `vergil verify --intent` integration.
//!
//! Wires the CEGIS loop into the CLI: builds env-loaded LLM providers
//! (Anthropic synthesizer + OpenAI critic + Voyage embedder), constructs a
//! Halmos-backed [`VerifierDispatcher`] that materializes each
//! [`SpecCandidate`] as a `test/CegisProperties.t.sol` file and dispatches
//! through `vergil-core::portfolio::dispatch`, and serializes the resulting
//! [`CegisRun`] to `vergil-out/proof.json`.
//!
//! Failure modes are surfaced as typed `IntentError`s with the relevant
//! env var or path so the user can fix configuration quickly.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

use vergil_core::cegis::{
    CegisConfig, CegisLoop, CegisRun, IterationStats, VerifierDispatcher, VerifierVerdict,
};
use vergil_core::critique::{Critic, CritiqueConfig};
use vergil_core::diagnosis::{DiagnosisConfig, Diagnostician};
use vergil_core::portfolio::{dispatch, PortfolioConfig, Verdict};
use vergil_core::refinement::{RefinementConfig, Refiner};
use vergil_core::synthesis::{RetrievedHint, Source as CoreSource, SpecCandidate, StaticAnalysisSummary};
use vergil_core::telemetry::TelemetrySink;
use vergil_llm::anthropic::AnthropicClient;
use vergil_llm::openai::OpenAiClient;
use vergil_llm::trace::{default_env_secrets, TraceRecorder};
use vergil_llm::{LlmProvider, ProviderId};
use vergil_proof::schema::{
    sha256_hex, Cost, CounterexampleSummary, ManifestValidationStatus, ProofArtifact,
    QualityMetrics, RunMeta, Source as ProofSource, SourceFile, Tier, ToolchainVersions,
    VerifiedProperty,
};
use vergil_properties::{
    Catalog, Embedder, MockEmbedder, RetrievalError, Retriever, VoyageEmbedder,
};

/// Errors surfaced by [`run_intent`]. Phase 4 Slice B3: every variant
/// carries enough context for an operator to know what was being
/// attempted (path, intent excerpt) and chain into the source error.
///
/// Operators inspect the chain via `anyhow::Error::chain` or
/// `std::error::Error::source` — every variant either holds the source
/// directly via `#[source]` or wraps it via `#[from]`.
#[derive(Debug, Error)]
pub enum IntentError {
    /// A required environment variable wasn't set. The variant carries
    /// the canonical name from the `VERGIL_*` family; check `.env` or
    /// the shell.
    #[error("missing env var {0} (set it in .env or your shell)")]
    MissingEnv(&'static str),

    /// Anthropic SDK initialization failed. The string is the SDK's
    /// error message verbatim — usually an invalid API key shape.
    #[error("anthropic client init: {0}")]
    Anthropic(String),

    /// Couldn't open the trace recorder at `path`. The most common
    /// cause is the parent dir being read-only.
    #[error("trace recorder open at {path}: {source}")]
    TraceOpen {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Project layout precondition failed (no `src/` dir, no `.sol`
    /// files in `src/`, no `foundry.toml`, etc.).
    #[error("project layout: {0}")]
    Project(String),

    /// Retrieval pipeline failed. The intent_preview is the first 80
    /// chars of the intent so logs can identify which run.
    #[error("retrieval failed (intent: {intent_preview}): {source}")]
    Retrieval {
        intent_preview: String,
        #[source]
        source: RetrievalError,
    },

    /// CEGIS loop returned an error. Source carries the typed cause
    /// (synthesis / critique / refinement / etc.).
    #[error("cegis loop: {source}")]
    Cegis {
        #[source]
        source: vergil_core::cegis::CegisError,
    },

    /// std::io::Error from any of the helper IO calls (mkdir, read,
    /// write). `#[from]` keeps the conversion ergonomic at call sites.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization of proof.json or the spec candidates file
    /// failed. Effectively unreachable for our own types — surfaces
    /// only if someone introduces a type with a non-serializable field.
    #[error("serialization: {0}")]
    Serialize(String),
}

fn intent_preview(s: &str) -> String {
    let trimmed: String = s.chars().take(80).collect();
    if s.chars().count() > 80 {
        format!("{trimmed}…")
    } else {
        trimmed
    }
}

/// Bundle of provider Arcs the CEGIS loop needs.
pub struct ProviderBundle {
    pub synthesizer: Arc<dyn LlmProvider>,
    pub critic: Arc<dyn LlmProvider>,
    pub diagnostician: Arc<dyn LlmProvider>,
    pub refiner: Arc<dyn LlmProvider>,
    pub embedder: Box<dyn Embedder>,
    pub synth_provider_id: ProviderId,
}

/// Build the production provider bundle from env vars. Anthropic is
/// required (synthesis). OpenAI is preferred for cross-provider critique
/// but falls back to Anthropic-on-Anthropic with a warning if not set.
/// Voyage is preferred for embeddings; falls back to MockEmbedder for a
/// degraded retrieval pass if not set.
pub fn build_providers_from_env(
    tracer: Option<TraceRecorder>,
) -> Result<ProviderBundle, IntentError> {
    let anthropic_key = env_or_missing(&["VERGIL_ANTHROPIC_API_KEY", "ANTHROPIC_API_KEY"])
        .ok_or(IntentError::MissingEnv("VERGIL_ANTHROPIC_API_KEY"))?;
    let mut anthropic =
        AnthropicClient::new(&anthropic_key).map_err(|e| IntentError::Anthropic(format!("{e}")))?;
    if let Some(rec) = tracer.clone() {
        anthropic = anthropic.with_tracer(rec);
    }
    let anthropic_arc: Arc<dyn LlmProvider> = Arc::new(anthropic);

    let critic: Arc<dyn LlmProvider> =
        match env_or_missing(&["VERGIL_OPENAI_API_KEY", "OPENAI_API_KEY"]) {
            Some(key) => {
                let mut openai = OpenAiClient::new(&key);
                if let Some(rec) = tracer.clone() {
                    openai = openai.with_tracer(rec);
                }
                Arc::new(openai)
            }
            None => {
                tracing::warn!(
                    "VERGIL_OPENAI_API_KEY not set — critique falls back to same-provider \
                     (vacuity defense weaker)"
                );
                anthropic_arc.clone()
            }
        };

    let embedder: Box<dyn Embedder> = match env_or_missing(&["VOYAGE_API_KEY"]) {
        Some(key) => Box::new(VoyageEmbedder::new(key)),
        None => {
            tracing::warn!(
                "VOYAGE_API_KEY not set — retrieval falls back to MockEmbedder (degraded mode)"
            );
            Box::new(MockEmbedder::new("mock-1024", 1024))
        }
    };

    Ok(ProviderBundle {
        synthesizer: anthropic_arc.clone(),
        critic,
        diagnostician: anthropic_arc.clone(),
        refiner: anthropic_arc,
        embedder,
        synth_provider_id: ProviderId::Anthropic,
    })
}

fn env_or_missing(keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// Halmos-backed dispatcher. Materializes each [`SpecCandidate`] as a
/// fresh `test/CegisProperties.t.sol` file in the Foundry project, then
/// runs the portfolio.
///
/// The contract scaffold (`pragma`, import, contract declaration, ctor)
/// is supplied at construction time so each project can plug in the
/// right import path and constructor invocation.
pub struct HalmosDispatcher {
    project: PathBuf,
    /// Solidity scaffold with a `{{CHECK_FN}}` placeholder where the
    /// SpecCandidate.halmos source goes. The contract must be named
    /// `CegisProperties` so the test file matches.
    scaffold: String,
    /// Wall-clock budget per portfolio dispatch.
    budget: Duration,
    /// Optional smtchecker source override (defaults to first .sol under src/).
    smtchecker_source: Option<PathBuf>,
}

impl HalmosDispatcher {
    pub fn new(project: PathBuf, scaffold: String, budget: Duration) -> Self {
        Self {
            project,
            scaffold,
            budget,
            smtchecker_source: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_smtchecker_source(mut self, p: PathBuf) -> Self {
        self.smtchecker_source = Some(p);
        self
    }

    fn render(&self, spec: &SpecCandidate) -> String {
        self.scaffold
            .replace("{{CHECK_FN}}", &spec.halmos)
            .replace("{{NAME}}", &spec.name)
    }

    fn default_smtchecker_source(&self) -> PathBuf {
        if let Some(p) = &self.smtchecker_source {
            return p.clone();
        }
        let src = self.project.join("src");
        if let Ok(entries) = std::fs::read_dir(&src) {
            for e in entries.flatten() {
                if e.path().extension().map(|s| s == "sol").unwrap_or(false) {
                    return e.path();
                }
            }
        }
        src.join("Token.sol")
    }
}

#[async_trait]
impl VerifierDispatcher for HalmosDispatcher {
    async fn dispatch(&self, spec: &SpecCandidate) -> VerifierVerdict {
        let test_dir = self.project.join("test");
        if let Err(e) = std::fs::create_dir_all(&test_dir) {
            return VerifierVerdict::Error {
                detail: format!("mkdir {}: {e}", test_dir.display()),
            };
        }
        let file = test_dir.join("CegisProperties.t.sol");
        let body = self.render(spec);
        if let Err(e) = std::fs::write(&file, body) {
            return VerifierVerdict::Error {
                detail: format!("write {}: {e}", file.display()),
            };
        }
        let cfg = PortfolioConfig {
            project: self.project.clone(),
            property: spec.name.clone(),
            smtchecker_source: self.default_smtchecker_source(),
            budget: self.budget,
            capture_smt_queries: true,
            // Persist .smt2 files under <project>/vergil-out/smt/ so
            // `vergil prove --solver <name>` can re-dispatch later.
            smt_persist_dir: Some(self.project.join("vergil-out").join("smt")),
        };
        let result = dispatch(cfg).await;
        match result.verdict {
            Verdict::Verified {
                backend,
                smt_query_sha256,
                ..
            } => VerifierVerdict::Verified {
                backend: backend_label(backend),
                smt_query_sha256,
            },
            Verdict::Counterexample {
                property, message, ..
            } => VerifierVerdict::Counterexample {
                message: format!("{property}: {message}"),
            },
            Verdict::Unknown { backends } => VerifierVerdict::Unknown {
                detail: summarize_backends(&backends),
            },
            Verdict::Error { backends } => VerifierVerdict::Error {
                detail: summarize_backends(&backends),
            },
        }
    }
}

fn summarize_backends(backends: &[vergil_core::portfolio::BackendOutcome]) -> String {
    backends
        .iter()
        .map(|b| format!("{:?}={}", b.backend, b.detail))
        .collect::<Vec<_>>()
        .join("; ")
}

fn backend_label(b: vergil_core::portfolio::Backend) -> String {
    match b {
        vergil_core::portfolio::Backend::Halmos => "halmos".to_string(),
        vergil_core::portfolio::Backend::SmtChecker => "smtchecker".to_string(),
    }
}

/// Inputs to [`run_intent`] — split out so call sites stay readable.
pub struct IntentRun {
    pub project: PathBuf,
    pub intent: String,
    /// Property-specific statement (kill criterion / batched per-property
    /// runs). When set, the critic scores against this narrower target
    /// instead of the contract-level intent. `None` for free-form
    /// `vergil verify --intent` invocations.
    pub description: Option<String>,
    pub scaffold: String,
    pub catalog: Catalog,
    pub cegis: CegisConfig,
    /// Override for the critic's per-axis minimum score. Defaults to the
    /// `CritiqueConfig` default when `None`. Kill criterion passes a
    /// looser 0.4 here so the prompt and runner stay in sync.
    pub min_critique_axis: Option<f32>,
    pub mutation_min: f64,
    pub budget_per_property: Duration,
    /// Telemetry sink (Phase 4 Slice B2). Defaults to [`CegisLoop::null_sink`]
    /// when callers don't pass one; the CLI passes a JsonlSink when
    /// `--telemetry-json <path>` is set so V2's billing layer can replay
    /// the JSONL stream.
    pub telemetry: Arc<dyn TelemetrySink>,
}

/// Orchestrate one full CEGIS run end-to-end and write `vergil-out/proof.json`.
///
/// On success, returns the [`CegisRun`] plus the path to the written proof.
///
/// # Errors
///
/// Returns an [`IntentError`] for any failure in the pipeline:
///
/// * [`IntentError::MissingEnv`] — required `VERGIL_ANTHROPIC_API_KEY` not set.
/// * [`IntentError::Anthropic`] — Anthropic SDK rejected the key shape.
/// * [`IntentError::TraceOpen`] — couldn't open the trace recorder (carries
///   the path).
/// * [`IntentError::Project`] — `src/` is missing or has no `.sol` files.
/// * [`IntentError::Retrieval`] — embedding API failure or empty corpus
///   (carries an 80-char preview of the intent string).
/// * [`IntentError::Cegis`] — the CEGIS loop itself failed (synthesis,
///   critique, or refinement). The chained [`vergil_core::cegis::CegisError`]
///   carries the typed cause.
/// * [`IntentError::Io`] — any filesystem operation (mkdir, read, write).
/// * [`IntentError::Serialize`] — proof.json / candidates.json serialization
///   failed (effectively unreachable for our own types).
pub async fn run_intent(spec: IntentRun) -> Result<(CegisRun, PathBuf), IntentError> {
    // V1.5 Phase 6 Slice 4: the intent path uses the shared layout
    // helper so the tier-aware tree is consistent with everything
    // Slice 8 layers on top.
    crate::output::layout::ensure_tree(&spec.project)?;
    let out_dir = crate::output::layout::vergil_out(&spec.project);
    std::fs::create_dir_all(out_dir.join("spec"))?;

    let tracer = TraceRecorder::open(&out_dir, default_env_secrets())
        .await
        .map_err(|source| IntentError::TraceOpen {
            path: out_dir.clone(),
            source,
        })?;

    let providers = build_providers_from_env(Some(tracer.clone()))?;

    // Static analysis: read the first .sol in src/ for a summary blob the
    // synthesize prompt can use. Real analysis (slither + solc layout)
    // happens later for manifest validation.
    let primary_source = first_solidity_source(&spec.project)?;
    // Phase 4 Slice A4: contract_source carries the concatenation of every
    // .sol under src/, so the synth prompt sees the full multi-contract
    // surface. Single-contract projects collapse to the same string as
    // before. Separators are clearly marked so the LLM parses each unit.
    let contract_source = combine_solidity_sources(&spec.project)?;

    let sa_summary = build_static_analysis_summary(&primary_source).await?;

    // Phase 4 Slice A1: classify the target contract's interface(s) so
    // retrieval can filter out cross-standard templates. Without this, an
    // ERC-721 intent pulls the catalog's dominant ERC-20 templates (shared
    // transfer/approve/balance vocabulary) and the synthesizer follows
    // template gravity into the wrong standard — the documented cause of the
    // ERC-721 kill-criterion stragglers. Empty result → no filtering.
    let detected_interfaces = vergil_solidity::signatures::detect_interfaces(&contract_source);

    // Retrieval: pull top-k templates that match the intent, restricted to
    // the detected interface(s).
    let cache_dir = out_dir.join("retrieval-cache");
    let retriever = Retriever::new(spec.catalog, providers.embedder, &cache_dir)
        .await
        .map_err(|source| IntentError::Retrieval {
            intent_preview: intent_preview(&spec.intent),
            source,
        })?;
    let hits = retriever
        .retrieve_for_interfaces(&spec.intent, 5, &detected_interfaces)
        .await
        .map_err(|source| IntentError::Retrieval {
            intent_preview: intent_preview(&spec.intent),
            source,
        })?;
    let retrieved: Vec<RetrievedHint> = hits
        .into_iter()
        .filter_map(|h| {
            retriever.template(&h.template_id).map(|t| RetrievedHint {
                template_id: t.manifest.id.clone(),
                description: t.manifest.description.clone(),
                halmos_snippet: t.halmos_source.chars().take(800).collect(),
            })
        })
        .collect();

    // Build the CEGIS loop.
    let mut critique_cfg = CritiqueConfig::default_for_openai();
    if let Some(min_axis) = spec.min_critique_axis {
        critique_cfg.min_axis = min_axis;
    }
    let critic = Critic::new(
        providers.critic.clone(),
        providers.synth_provider_id,
        critique_cfg,
    );
    let diagnostician = Diagnostician::new(
        providers.diagnostician.clone(),
        DiagnosisConfig::default_for_anthropic(),
    );
    let refiner = Refiner::new(
        providers.refiner.clone(),
        RefinementConfig::default_for_anthropic(),
    );
    let dispatcher: Arc<dyn VerifierDispatcher> = Arc::new(HalmosDispatcher::new(
        spec.project.clone(),
        spec.scaffold.clone(),
        spec.budget_per_property,
    ));
    let cegis = CegisLoop {
        synthesizer: providers.synthesizer.clone(),
        critic,
        diagnostician,
        refiner,
        mutation_gate: None,
        dispatcher,
        cfg: spec.cegis.clone(),
        mutation_min: spec.mutation_min,
        telemetry: spec.telemetry.clone(),
    };

    let started = std::time::Instant::now();
    // Phase 4 Slice A3: extract the contract's external/public function
    // signatures and inject as `available_methods` so the synthesizer
    // stops hallucinating methods or reaching for the wrong interface.
    let signatures = vergil_solidity::signatures::extract(&contract_source);
    let available_methods = vergil_solidity::signatures::render_available_methods(&signatures);
    let run = cegis
        .run_with_description(
            &spec.intent,
            spec.description.as_deref(),
            &available_methods,
            &sa_summary,
            &retrieved,
            &contract_source,
            &spec.scaffold,
        )
        .await
        .map_err(|source| IntentError::Cegis { source })?;
    let wall_clock_ms = started.elapsed().as_millis() as u64;

    // Serialize CegisRun → ProofArtifact.
    let proof = build_proof_artifact(&spec.project, &spec.intent, &run, wall_clock_ms)?;
    let proof_path = crate::output::layout::top_level_proof_json(&spec.project);
    let json =
        serde_json::to_string_pretty(&proof).map_err(|e| IntentError::Serialize(format!("{e}")))?;
    std::fs::write(&proof_path, json)?;

    // Persist spec drafts and critiques for the report.
    let spec_path = out_dir.join("spec").join("candidates.json");
    let spec_json =
        serde_json::to_string_pretty(&run).map_err(|e| IntentError::Serialize(format!("{e}")))?;
    std::fs::write(spec_path, spec_json)?;

    Ok((run, proof_path))
}

fn first_solidity_source(project: &Path) -> Result<PathBuf, IntentError> {
    let src = project.join("src");
    let entries = std::fs::read_dir(&src)
        .map_err(|e| IntentError::Project(format!("read {}: {e}", src.display())))?;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().map(|s| s == "sol").unwrap_or(false) {
            return Ok(p);
        }
    }
    Err(IntentError::Project(format!(
        "no .sol files found under {}",
        src.display()
    )))
}

/// Phase 4 Slice A4: read every `.sol` under `<project>/src/` and join
/// their bodies with file-name delimiters so the synth prompt sees the
/// whole multi-contract surface. Single-contract projects produce
/// effectively the same string they did before (plus a one-line header).
/// Returns the concatenated source.
fn combine_solidity_sources(project: &Path) -> Result<String, IntentError> {
    let src = project.join("src");
    let entries = std::fs::read_dir(&src)
        .map_err(|e| IntentError::Project(format!("read {}: {e}", src.display())))?;
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|s| s == "sol").unwrap_or(false))
        .collect();
    if files.is_empty() {
        return Err(IntentError::Project(format!(
            "no .sol files found under {}",
            src.display()
        )));
    }
    files.sort();
    let mut out = String::new();
    for path in &files {
        let body = std::fs::read_to_string(path)?;
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        out.push_str(&format!("// ── src/{name} ──\n"));
        out.push_str(&body);
        if !body.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

async fn build_static_analysis_summary(
    source: &Path,
) -> Result<StaticAnalysisSummary, IntentError> {
    // Best-effort: if static analysis fails (slither missing in some envs),
    // fall back to a textual hint from the file itself. SPEC §3.3 lets
    // synthesis run in degraded mode without a full analysis pass.
    match vergil_solidity::static_analysis::analyze(source, Duration::from_secs(60)).await {
        Ok(report) => {
            let mut text = String::new();
            for layout in &report.storage {
                text.push_str(&format!("contract {}:\n", layout.qualified_name));
                for entry in &layout.entries {
                    text.push_str(&format!(
                        "  slot {} = {} : {}\n",
                        entry.slot, entry.label, entry.type_id
                    ));
                }
            }
            if !report.slither.detectors.is_empty() {
                text.push_str("\nslither detectors:\n");
                for d in &report.slither.detectors {
                    text.push_str(&format!(
                        "  [{}] {}: {}\n",
                        d.impact, d.check, d.description
                    ));
                }
            }
            Ok(StaticAnalysisSummary { text })
        }
        Err(e) => {
            tracing::warn!(
                "static analysis failed, continuing in degraded mode: {e}; \
                 prompt will fall back to source-only context"
            );
            Ok(StaticAnalysisSummary {
                text: format!("(static analysis unavailable: {e})"),
            })
        }
    }
}

fn build_proof_artifact(
    project: &Path,
    intent: &str,
    run: &CegisRun,
    wall_clock_ms: u64,
) -> Result<ProofArtifact, IntentError> {
    // Hash every .sol under src/ so vergil prove can re-verify the source.
    let mut source_files = Vec::new();
    let src_dir = project.join("src");
    if src_dir.is_dir() {
        for e in std::fs::read_dir(&src_dir)?.flatten() {
            let p = e.path();
            if p.extension().map(|s| s == "sol").unwrap_or(false) {
                let bytes = std::fs::read(&p)?;
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
    if source_files.is_empty() {
        return Err(IntentError::Project(
            "no source files to hash in src/".into(),
        ));
    }

    let verified_properties: Vec<VerifiedProperty> = run
        .outcomes
        .iter()
        .filter_map(|o| match &o.verifier_verdict {
            VerifierVerdict::Verified {
                backend,
                smt_query_sha256,
            } => Some(VerifiedProperty {
                name: o.candidate.name.clone(),
                backend: backend.clone(),
                spec_sha256: sha256_hex(o.candidate.halmos.as_bytes()),
                template_ref: o.candidate.template_ref.clone(),
                wall_clock_ms: total_wall_clock(&run.iterations),
                smt_query_sha256: smt_query_sha256.clone(),
                manifest_validation: ManifestValidationStatus {
                    storage_ok: true,
                    modifiers_ok: true,
                    external_calls_ok: true,
                    warnings: o.manifest_warnings.clone(),
                },
                source: bridge_source(o.candidate.source),
                tier: bridge_tier(o.candidate.source),
            }),
            _ => None,
        })
        .collect();

    let counterexamples: Vec<CounterexampleSummary> = run
        .outcomes
        .iter()
        .filter_map(|o| match &o.verifier_verdict {
            VerifierVerdict::Counterexample { message } => Some(CounterexampleSummary {
                property: o.candidate.name.clone(),
                backend: "halmos".to_string(),
                cex_file: format!("counterexamples/Cex_{}.t.sol", o.candidate.name),
                wall_clock_ms: 0,
                trace_summary: message.clone(),
            }),
            _ => None,
        })
        .collect();

    let tokens_in: u32 = run.iterations.iter().map(|i| i.tokens_in).sum();
    let tokens_out: u32 = run.iterations.iter().map(|i| i.tokens_out).sum();
    let total_synth: usize = run.iterations.iter().map(|i| i.synthesized).sum();
    let total_dropped: usize = run.iterations.iter().map(|i| i.dropped_critique).sum();
    let critique_pass_rate = if total_synth == 0 {
        0.0
    } else {
        1.0 - (total_dropped as f32 / total_synth as f32)
    };

    Ok(ProofArtifact {
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
            mutation_coverage_min: None,
            critique_pass_rate,
            mutation_testing_enabled: false,
        },
        cost: Cost {
            tokens_in,
            tokens_out,
            usd_estimate: run.total_cost_usd,
            wall_clock_ms,
        },
    })
}

fn total_wall_clock(iters: &[IterationStats]) -> u64 {
    iters.iter().map(|i| i.wall_clock_ms).sum()
}

/// Default scaffold for the examples/erc20 reference contract. The kill
/// criterion runner will compose project-specific scaffolds per OZ contract.
pub fn default_scaffold_for_erc20() -> String {
    r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Token} from "../src/Token.sol";

contract CegisProperties {
    Token internal token;

    constructor() {
        token = new Token(1_000_000 ether);
    }

    {{CHECK_FN}}
}
"#
    .to_string()
}

/// Locate `<repo>/crates/vergil-properties/templates/` relative to the
/// vergil binary at runtime. Searches CARGO_MANIFEST_DIR first (dev), then
/// walks up from CWD looking for the templates dir (release / installed).
pub fn locate_templates_dir() -> Option<PathBuf> {
    if let Some(mf) = option_env!("CARGO_MANIFEST_DIR") {
        let p = Path::new(mf)
            .join("..")
            .join("vergil-properties")
            .join("templates");
        if p.is_dir() {
            return Some(p);
        }
    }
    let mut cwd = std::env::current_dir().ok()?;
    for _ in 0..6 {
        let p = cwd
            .join("crates")
            .join("vergil-properties")
            .join("templates");
        if p.is_dir() {
            return Some(p);
        }
        if !cwd.pop() {
            break;
        }
    }
    None
}

/// Bridge `vergil_core::synthesis::Source` to its on-disk twin in
/// `vergil_proof::schema::Source`. The match is exhaustive on the
/// core enum so a future variant added without a proof-side mirror is
/// a compile error, not a silent downgrade to `UserIntent`. SPEC §3.6.
///
/// Same helper lives in `commands/verify.rs`; lib.rs uses
/// `#[path = "commands/intent.rs"] pub mod intent;` so a shared
/// `commands::` helper is reachable from the binary build but not from
/// the lib build. Inlining the bridge keeps both build contexts happy
/// without a separate top-level utility module.
fn bridge_source(core: CoreSource) -> ProofSource {
    match core {
        CoreSource::UserIntent => ProofSource::UserIntent,
        CoreSource::AttackCatalog => ProofSource::AttackCatalog,
        CoreSource::Conformance => ProofSource::Conformance,
        CoreSource::Tests => ProofSource::Tests,
        CoreSource::NatSpec => ProofSource::NatSpec,
        CoreSource::Structural => ProofSource::Structural,
    }
}

fn bridge_tier(core: CoreSource) -> Tier {
    match core {
        CoreSource::UserIntent => Tier::Intent,
        CoreSource::AttackCatalog
        | CoreSource::Conformance
        | CoreSource::Tests
        | CoreSource::NatSpec
        | CoreSource::Structural => Tier::ZeroConfig,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_preview_truncates_long_intents_with_ellipsis() {
        let long_input: String = "x".repeat(120);
        let preview = intent_preview(&long_input);
        let len = preview.chars().count();
        // 80 chars + ellipsis = 81 chars.
        assert_eq!(len, 81);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn intent_preview_returns_short_intents_unchanged() {
        let input = "verify totalSupply is preserved";
        assert_eq!(intent_preview(input), input);
        assert!(!intent_preview(input).ends_with('…'));
    }

    #[test]
    fn intent_error_trace_open_carries_path_and_source() {
        let e = IntentError::TraceOpen {
            path: PathBuf::from("/locked/dir"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        };
        let msg = format!("{e}");
        assert!(msg.contains("/locked/dir"), "{msg}");
        assert!(msg.contains("denied"), "{msg}");
        // source() returns the wrapped io::Error so callers can chain.
        use std::error::Error as _;
        assert!(e.source().is_some());
    }

    #[test]
    fn intent_error_retrieval_carries_intent_preview() {
        let e = IntentError::Retrieval {
            intent_preview: "verify token transfer is monotonic".to_string(),
            source: RetrievalError::DimMismatch {
                embedder: 1024,
                cache: 512,
            },
        };
        let msg = format!("{e}");
        assert!(msg.contains("verify token transfer"), "{msg}");
        assert!(msg.contains("retrieval failed"), "{msg}");
    }

    #[test]
    fn render_substitutes_check_fn_placeholder() {
        let d = HalmosDispatcher::new(
            PathBuf::from("/tmp"),
            "contract X { {{CHECK_FN}} }".to_string(),
            Duration::from_secs(1),
        );
        let spec = SpecCandidate {
            name: "check_x".to_string(),
            halmos: "function check_x() public {}".to_string(),
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: true,
            source: vergil_core::synthesis::Source::UserIntent,
            intent_text: None,
        };
        let s = d.render(&spec);
        assert!(s.contains("function check_x() public {}"));
        assert!(!s.contains("{{CHECK_FN}}"));
    }

    #[test]
    fn render_substitutes_name_placeholder() {
        let d = HalmosDispatcher::new(
            PathBuf::from("/tmp"),
            "// {{NAME}}\n{{CHECK_FN}}".to_string(),
            Duration::from_secs(1),
        );
        let spec = SpecCandidate {
            name: "the_check".to_string(),
            halmos: "function the_check() public {}".to_string(),
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: true,
            source: vergil_core::synthesis::Source::UserIntent,
            intent_text: None,
        };
        let s = d.render(&spec);
        assert!(s.contains("// the_check"));
        assert!(s.contains("function the_check"));
    }

    #[test]
    fn build_providers_errors_without_anthropic_key() {
        // Snapshot the env so we don't leak state into other tests.
        let prev_a = std::env::var("VERGIL_ANTHROPIC_API_KEY").ok();
        let prev_b = std::env::var("ANTHROPIC_API_KEY").ok();
        std::env::remove_var("VERGIL_ANTHROPIC_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");
        let r = build_providers_from_env(None);
        assert!(matches!(r, Err(IntentError::MissingEnv(_))));
        if let Some(v) = prev_a {
            std::env::set_var("VERGIL_ANTHROPIC_API_KEY", v);
        }
        if let Some(v) = prev_b {
            std::env::set_var("ANTHROPIC_API_KEY", v);
        }
    }

    #[test]
    fn locate_templates_dir_finds_the_corpus() {
        // In tree, this should always succeed.
        let p = locate_templates_dir().expect("templates dir locatable");
        assert!(p.join("erc20-sum-of-balances").is_dir());
    }
}
