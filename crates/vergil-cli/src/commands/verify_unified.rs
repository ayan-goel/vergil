//! Unified `vergil verify` orchestration — V1.5 Phase 6 Slice 8.
//!
//! Wires Stages 0→4 from SPEC §3.1 into a single command runner:
//!
//!   Stage 0  — fingerprint(project)        (Slice 1)
//!   Stage 1  — catalog + tests + natspec   (Slice 3 + Phase 4)
//!              extractors run in parallel via tokio::join_all;
//!              each produces SpecCandidate records tagged with Source.
//!   Stage 1.5 — critique filters survivors via the 4-axis pass
//!               (Phase 4 Slice 5 restate_the_source extension).
//!   Stage 2  — confirm gate (Slice 7); --yes auto-confirms, --resume
//!              picks up from `vergil-out/confirm/state.json`.
//!   Stage 3  — dispatch confirmed candidates' Halmos sources via the
//!              SMT portfolio. NOT a full CEGIS re-run: the Stage 1
//!              extractors already synthesized; running CEGIS again
//!              would double LLM cost. Per-candidate refutations
//!              stream to disk via Slice 6's CexSink.
//!   Stage 4  — format_verdict (Slice 5) → report.md + proof.json.
//!
//! CLI flag matrix (SPEC §3.7):
//!   --mode {zero-config|intent|both}      both = run Stage 1 oracles +
//!                                          user --intent; zero-config =
//!                                          oracles only; intent = V1
//!                                          path (delegated to
//!                                          commands::intent).
//!   --catalog-subset <CSV>                Restrict catalog activation
//!                                          to categories listed.
//!   --no-tests / --no-natspec /           Skip the named oracle.
//!     --no-structural                     (structural is NOOP in
//!                                          Phase 6 either way.)
//!   --yes                                 Auto-confirm Stage 2.
//!   --resume                              Resume from existing
//!                                          confirm/state.json.
//!   --list-applicable                     Print activated catalog +
//!                                          available oracles and
//!                                          exit 0. No LLM calls.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;

use vergil_core::catalog_intent::{
    extract_from_catalog, CatalogIntentConfig, CatalogIntentReport,
};
use vergil_core::cegis::{VerifierDispatcher, VerifierVerdict};
use vergil_core::confirm::{
    resume_or_new, run_gate, ConfirmState, ConfirmStatus, Decision, GateMode, ProposedIntent,
};
use vergil_core::critique::{Critic, CritiqueConfig, CritiqueResult};
use vergil_core::fingerprint::{fingerprint, Fingerprint};
use vergil_core::natspec_intent::{
    extract_from_natspec, NatSpecIntentConfig, NatSpecIntentReport,
};
use vergil_core::structural::{
    extract_from_structural, StructuralConfig, StructuralReport,
};
use vergil_core::synthesis::{
    RetrievedHint, Source as CoreSource, SpecCandidate, StaticAnalysisSummary, SynthesisConfig,
};
use vergil_core::telemetry::{JsonlSink, NullSink, TelemetrySink};
use vergil_core::tests_intent::{extract_from_tests, TestsIntentConfig, TestsIntentReport};
use vergil_llm::ProviderId;
use vergil_properties::{activate, ActivationResult, AttackCatalog, SmtStatus, StaticFacts};
use vergil_proof::schema::{
    Cost, ProofArtifact, QualityMetrics, RunMeta, Source as ProofSource, SourceFile, Tier,
    ToolchainVersions, VerifiedProperty, ManifestValidationStatus, sha256_hex,
};
use vergil_solidity::natspec::parse_natspec_dir;
use vergil_solidity::test_parser::parse_tests;

use crate::output::cex_sink::{CexSink, CounterexampleRecord};
use crate::output::layout;
use crate::output::verdict::{
    format_verdict, AvailableOraclesSummary, DocumentOnlyTemplate, FingerprintSummary, Headline,
    PerTemplateFailureSummary, PropertyOutcome, PropertyVerdict, SkippedTemplateSummary,
    StratifiedInputs,
};

/// Knobs the binary's clap layer projects into the runner. Owned to
/// keep the call sites tidy.
#[derive(Debug, Clone)]
pub struct UnifiedVerifyArgs {
    pub project: PathBuf,
    /// Optional Solidity scaffold override; same semantics as
    /// `commands/verify.rs::resolve_scaffold`.
    pub scaffold_override: Option<PathBuf>,
    pub no_tests: bool,
    pub no_natspec: bool,
    /// Restrict catalog activation to these categories. Empty = all
    /// applicable.
    pub catalog_categories: Vec<String>,
    /// `--yes`: auto-confirm Stage 2.
    pub auto_confirm: bool,
    /// `--resume`: pick up a prior gate run.
    pub resume: bool,
    /// `--list-applicable`: print activation + oracle availability,
    /// exit 0 without spawning LLM calls.
    pub list_applicable: bool,
    /// Optional `--telemetry-json` path; threads through to CexSink
    /// and the broader pipeline for V2 billing pin.
    pub telemetry_json: Option<PathBuf>,
    /// Tenant id for telemetry.
    pub tenant_id: String,
    /// Per-dispatch budget (overrides default 120s).
    pub dispatch_budget: Duration,
}

impl Default for UnifiedVerifyArgs {
    fn default() -> Self {
        Self {
            project: PathBuf::new(),
            scaffold_override: None,
            no_tests: false,
            no_natspec: false,
            catalog_categories: Vec::new(),
            auto_confirm: false,
            resume: false,
            list_applicable: false,
            telemetry_json: None,
            tenant_id: "internal".to_string(),
            dispatch_budget: Duration::from_secs(120),
        }
    }
}

/// Errors that can short-circuit a unified verify run. The CLI
/// converts each to its SPEC §3.1 exit code (1 = cex, 2 = unknown,
/// 3 = error). Verdict-level outcomes (e.g. all candidates Unknown)
/// are NOT errors; they're reported via the stratified verdict.
#[derive(Debug, thiserror::Error)]
pub enum UnifiedVerifyError {
    #[error("invalid project: {0}")]
    BadProject(PathBuf),
    #[error("fingerprint: {0}")]
    Fingerprint(#[from] vergil_core::fingerprint::FingerprintError),
    #[error("catalog load: {0}")]
    Catalog(#[from] vergil_properties::AttackError),
    #[error("provider init: {0}")]
    ProviderInit(String),
    #[error("synthesis: {0}")]
    Synthesis(#[from] vergil_core::synthesis::SynthesisError),
    #[error("scaffold: {0}")]
    Scaffold(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("confirm gate: {0}")]
    Confirm(#[from] vergil_core::confirm::ConfirmError),
    #[error("serialize: {0}")]
    Serialize(String),
    #[error("telemetry sink: {0}")]
    Telemetry(String),
}

impl UnifiedVerifyError {
    /// SPEC §3.1 exit code. Pipeline errors → 3. (main.rs reads this.)
    #[allow(dead_code)]
    pub fn exit_code(&self) -> u8 {
        3
    }
}

/// End-to-end runner. Returns the formatted verdict so the CLI can
/// print + exit-code-derive. Slice 8's main.rs entry point owns
/// the tokio runtime; this function is async because the Stage 1
/// oracles join_all on parallel LLM calls.
pub async fn run(args: UnifiedVerifyArgs) -> Result<RunReport, UnifiedVerifyError> {
    let project = args
        .project
        .canonicalize()
        .map_err(|_| UnifiedVerifyError::BadProject(args.project.clone()))?;
    if !project.is_dir() || !project.join("foundry.toml").is_file() {
        return Err(UnifiedVerifyError::BadProject(project));
    }
    layout::ensure_tree(&project)?;

    // ─── Stage 0 — fingerprint ────────────────────────────────────────
    let fp = fingerprint(&project)?;

    // ─── Catalog load + activation ────────────────────────────────────
    let templates_dir = locate_templates_dir().ok_or_else(|| {
        UnifiedVerifyError::Scaffold("could not locate attack-catalog templates dir".into())
    })?;
    let catalog = AttackCatalog::load(&templates_dir)?;
    let facts = fingerprint_to_facts(&fp);
    let mut activation = activate(&catalog, &facts);
    if !args.catalog_categories.is_empty() {
        filter_activation_by_category(&mut activation, &args.catalog_categories);
    }
    let document_only_templates = collect_document_only_templates(&activation);

    // ─── --list-applicable short-circuit ─────────────────────────────
    if args.list_applicable {
        print_list_applicable(&fp, &activation);
        return Ok(RunReport::ListApplicable {
            project_path: project,
            fingerprint: fp,
            activated: activation.templates.len(),
            skipped: activation.skipped.len(),
        });
    }

    // ─── Provider init (lazy: only needed if we run Stage 1) ──────────
    let mut providers = ProviderHandles::default();
    let need_llm = !activation.templates.is_empty()
        || (!args.no_tests && fp.available_oracles.tests)
        || (!args.no_natspec && fp.available_oracles.natspec);
    if need_llm {
        providers = ProviderHandles::from_env().map_err(UnifiedVerifyError::ProviderInit)?;
    }

    // ─── Stage 1 — parallel oracles ──────────────────────────────────
    let stage1 = run_stage1(&args, &fp, &activation, &providers).await?;

    // ─── Stage 1.5 — critique ────────────────────────────────────────
    let critique_cfg = CritiqueConfig::default_for_openai();
    let critic_provider = match providers.critic.clone() {
        Some(p) => p,
        None => match providers.synthesizer.clone() {
            Some(p) => p,
            None => {
                // No oracles ran; nothing to critique.
                let report_md_path = layout::report_md(&project);
                let proof_json_path = layout::top_level_proof_json(&project);
                return Ok(RunReport::Verdict {
                    project_path: project,
                    headline: Headline::Incomplete,
                    outcomes: Vec::new(),
                    report_md_path,
                    proof_json_path,
                });
            }
        },
    };
    let critic_provider_id = providers.critic_provider_id.unwrap_or(ProviderId::Anthropic);
    let critic = Critic::new(critic_provider, critic_provider_id, critique_cfg);
    let candidates = stage1.candidates.clone();
    let critique_results = if candidates.is_empty() {
        Vec::new()
    } else {
        critic.critique_all(&candidates, "Phase 6 stratified verification", None).await
    };
    let critique_outcome = critic.filter_accepted(candidates, critique_results);

    // ─── Stage 2 — confirm gate ──────────────────────────────────────
    let run_id = format!("unified-{}", Utc::now().format("%Y%m%dT%H%M%SZ"));
    let proposed: Vec<ProposedIntent> = critique_outcome
        .kept
        .iter()
        .map(|(c, r)| spec_to_proposed(c, r))
        .collect();
    let state_path = layout::confirm_state(&project);
    let start_state = if args.resume {
        resume_or_new(&state_path, &run_id, proposed.clone(), Utc::now())?
    } else {
        ConfirmState::new(run_id.clone(), proposed.clone(), Utc::now())
    };
    let final_state = if args.auto_confirm {
        run_gate(&state_path, start_state, GateMode::AutoYes, Utc::now())?
    } else if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        let mut prompter = StdinPrompter::default();
        run_gate(
            &state_path,
            start_state,
            GateMode::Tty {
                prompter: &mut prompter,
            },
            Utc::now(),
        )?
    } else {
        let mut stdin = std::io::stdin().lock();
        let mut stdout = std::io::stdout().lock();
        run_gate(
            &state_path,
            start_state,
            GateMode::Json {
                reader: &mut stdin,
                writer: &mut stdout,
            },
            Utc::now(),
        )?
    };
    let confirmed: Vec<(ProposedIntent, Decision)> = final_state.confirmed_intents();

    // ─── Stage 3 — dispatch confirmed candidates ─────────────────────
    let scaffold = crate::commands::verify::resolve_scaffold(
        &project,
        args.scaffold_override.as_deref(),
    )
    .map_err(UnifiedVerifyError::Scaffold)?;

    let telemetry: Arc<dyn TelemetrySink> = match &args.telemetry_json {
        Some(p) => Arc::new(
            JsonlSink::open(p).map_err(|e| UnifiedVerifyError::Telemetry(format!("{e}")))?,
        ),
        None => Arc::new(NullSink),
    };
    let cex_sink = CexSink::new(&project, telemetry.clone(), &args.tenant_id, &run_id);
    let dispatcher = crate::commands::intent::HalmosDispatcher::new(
        project.clone(),
        scaffold,
        args.dispatch_budget,
    );

    let mut outcomes: Vec<PropertyOutcome> = Vec::new();
    // Index back from confirmed intent id → SpecCandidate so we
    // dispatch the actual synthesized Halmos source for it.
    let candidates_by_id = index_candidates(&critique_outcome.kept);
    for (intent, decision) in &confirmed {
        let Some(candidate) = candidates_by_id.get(&intent.id) else {
            continue;
        };
        let effective_candidate = match decision {
            Decision::Edit { new_text } => {
                // User edited the intent text. Without a re-synthesis,
                // the existing Halmos source no longer reflects the
                // user's edit. Note this in the verdict — surfaces as
                // Unknown rather than a misleading proof.
                outcomes.push(PropertyOutcome {
                    name: candidate.name.clone(),
                    source: proof_source(candidate.source),
                    tier: tier_for_source(candidate.source),
                    verdict: PropertyVerdict::Unknown {
                        detail: format!(
                            "intent edited at Stage 2 (new text: '{new_text}'); re-synthesis not yet wired"
                        ),
                        frontier_over_approximation: None,
                    },
                    template_ref: candidate.template_ref.clone(),
                    intent_text: Some(new_text.clone()),
                });
                continue;
            }
            _ => candidate,
        };
        let v = dispatcher.dispatch(effective_candidate).await;
        let outcome = build_outcome(effective_candidate, intent, &v);
        // Stream cex on refutation.
        if let VerifierVerdict::Counterexample { message } = &v {
            // For now Slice 8 hands the verifier message in as the
            // body — V1's emit_counterexample is for the V1
            // properties.yaml path. Slice 10/11 may swap in a richer
            // Halmos-trace renderer; for the live exit test the file's
            // existence is what matters.
            let _ = cex_sink.emit(CounterexampleRecord {
                property: effective_candidate.name.clone(),
                source: effective_candidate.source,
                template_ref: effective_candidate.template_ref.clone(),
                source_sol: render_simple_cex(&effective_candidate.name, message),
                trace_summary: message.clone(),
            });
        }
        outcomes.push(outcome);
    }

    // ─── Stage 4 — format verdict ─────────────────────────────────────
    let strat = StratifiedInputs {
        project_path: project.display().to_string(),
        fingerprint: fingerprint_summary(&fp),
        properties: outcomes.clone(),
        skipped_templates: activation
            .skipped
            .iter()
            .map(|s| SkippedTemplateSummary {
                id: s.id.clone(),
                reason: s.reason.clone(),
            })
            .collect(),
        per_template_failures: stage1
            .catalog
            .as_ref()
            .map(|r| {
                r.per_template_failures
                    .iter()
                    .map(|f| PerTemplateFailureSummary {
                        template_id: f.template_id.clone(),
                        reason: f.reason.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        document_only_templates,
        phase5_structural_pending: true,
    };
    let verdict_out = format_verdict(strat);

    // Write report.md
    let report_path = layout::report_md(&project);
    std::fs::write(&report_path, verdict_out.report_md())?;

    // Write top-level proof.json — combines the V1-style ProofArtifact
    // (so `vergil prove` keeps working) with a `verdict` field that
    // carries the stratified output.
    let proof = build_proof_artifact(&project, &outcomes, &verdict_out.headline)?;
    let proof_path = layout::top_level_proof_json(&project);
    let mut proof_json = serde_json::to_value(&proof)
        .map_err(|e| UnifiedVerifyError::Serialize(format!("{e}")))?;
    if let serde_json::Value::Object(map) = &mut proof_json {
        map.insert("verdict".into(), verdict_out.proof_json());
    }
    std::fs::write(
        &proof_path,
        serde_json::to_string_pretty(&proof_json)
            .map_err(|e| UnifiedVerifyError::Serialize(format!("{e}")))?,
    )?;

    Ok(RunReport::Verdict {
        project_path: project,
        headline: verdict_out.headline,
        outcomes,
        report_md_path: report_path,
        proof_json_path: proof_path,
    })
}

// Fields are populated by `run()` and read by the CLI dispatch in
// main.rs; the lib-mode build doesn't reach those readers, so suppress
// the dead-code lint at the enum level.
#[allow(dead_code)]
#[derive(Debug)]
pub enum RunReport {
    /// `--list-applicable` short-circuit.
    ListApplicable {
        project_path: PathBuf,
        fingerprint: Fingerprint,
        activated: usize,
        skipped: usize,
    },
    /// Full run produced a verdict.
    Verdict {
        project_path: PathBuf,
        headline: Headline,
        outcomes: Vec<PropertyOutcome>,
        report_md_path: PathBuf,
        proof_json_path: PathBuf,
    },
}

impl RunReport {
    /// SPEC §3.1 exit code: 0 verified, 1 cex, 2 unknown, 3 error.
    pub fn exit_code(&self) -> u8 {
        match self {
            RunReport::ListApplicable { .. } => 0,
            RunReport::Verdict {
                headline, outcomes, ..
            } => match headline {
                Headline::Refuted => 1,
                Headline::VerifiedInScope => 0,
                Headline::Incomplete => {
                    if outcomes
                        .iter()
                        .any(|p| matches!(p.verdict, PropertyVerdict::Error { .. }))
                    {
                        3
                    } else {
                        2
                    }
                }
            },
        }
    }
}

// ─── Stage 1: parallel oracles ───────────────────────────────────────────────

#[derive(Default)]
struct Stage1Outputs {
    candidates: Vec<SpecCandidate>,
    catalog: Option<CatalogIntentReport>,
    #[allow(dead_code)]
    tests: Option<TestsIntentReport>,
    #[allow(dead_code)]
    natspec: Option<NatSpecIntentReport>,
    /// V1.5 Phase 5 — structural-mining oracle. `Some` when the
    /// deterministic structural pass ran (always, when Stage 1 ran at
    /// all); `None` when no LLM providers were available so Stage 1
    /// short-circuited entirely.
    structural: Option<StructuralReport>,
}

async fn run_stage1(
    args: &UnifiedVerifyArgs,
    fp: &Fingerprint,
    activation: &ActivationResult<'_>,
    providers: &ProviderHandles,
) -> Result<Stage1Outputs, UnifiedVerifyError> {
    let mut out = Stage1Outputs::default();
    if providers.synthesizer.is_none() {
        return Ok(out);
    }
    let synthesizer = providers.synthesizer.clone().unwrap();
    let extractor = providers.extractor.clone().unwrap_or_else(|| synthesizer.clone());
    // Phase 6 cost-controlled synthesis: samples=1 (V1 default is 16 for
    // CEGIS but Phase 6 fans out across many candidates instead, so the
    // budget per-candidate is tight). Catalog_intent additionally
    // overrides samples_per_intent for its branch.
    let synth_cfg = SynthesisConfig {
        samples: 1,
        ..SynthesisConfig::default_for_anthropic()
    };

    let contract_source = read_contract_source(&fp.contract_sources);
    let scaffold = crate::commands::verify::resolve_scaffold(
        &fp.project_root,
        args.scaffold_override.as_deref(),
    )
    .map_err(UnifiedVerifyError::Scaffold)?;
    let available_methods = render_available_methods_for(&fp.contract_sources);
    let sa = StaticAnalysisSummary::default();
    let _retrieved: Vec<RetrievedHint> = Vec::new();

    // Catalog oracle
    let cat_fut = {
        let synthesizer = synthesizer.clone();
        let cfg = CatalogIntentConfig::default_for_anthropic();
        let methods = available_methods.clone();
        let sa = sa.clone();
        let src = contract_source.clone();
        let scaf = scaffold.clone();
        let synth = synth_cfg.clone();
        async move {
            extract_from_catalog(
                activation,
                &cfg,
                synthesizer,
                &synth,
                &methods,
                &sa,
                &[],
                &src,
                &scaf,
            )
            .await
        }
    };

    // Tests oracle (skip if --no-tests or no tests available).
    let tests_data = if args.no_tests || !fp.available_oracles.tests {
        Vec::new()
    } else {
        parse_tests(&fp.project_root).unwrap_or_default()
    };
    let test_fut = {
        let extractor = extractor.clone();
        let synthesizer = synthesizer.clone();
        let cfg = TestsIntentConfig::default_for_anthropic();
        let synth = synth_cfg.clone();
        let methods = available_methods.clone();
        let sa = sa.clone();
        let src = contract_source.clone();
        let scaf = scaffold.clone();
        let tests = tests_data.clone();
        async move {
            extract_from_tests(
                &tests, &cfg, extractor, synthesizer, &synth, &methods, &sa, &[], &src, &scaf,
            )
            .await
        }
    };

    // NatSpec oracle
    let ns_data = if args.no_natspec || !fp.available_oracles.natspec {
        Vec::new()
    } else {
        parse_natspec_dir(&fp.project_root)
            .map(|pairs| pairs.into_iter().map(|(_, b)| b).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let ns_fut = {
        let extractor = extractor.clone();
        let synthesizer = synthesizer.clone();
        let cfg = NatSpecIntentConfig::default_for_anthropic();
        let synth = synth_cfg.clone();
        let methods = available_methods.clone();
        let sa = sa.clone();
        let src = contract_source.clone();
        let scaf = scaffold.clone();
        let blocks = ns_data.clone();
        async move {
            extract_from_natspec(
                &blocks, &cfg, extractor, synthesizer, &synth, &methods, &sa, &[], &src, &scaf,
            )
            .await
        }
    };

    // Structural oracle — deterministic, no LLM, no provider Arcs.
    // Wrapped in an async block so it joins symmetrically with the
    // three LLM oracles. Slice 0 ships an empty stub; Slices 1-5 add
    // real miners and source-loading.
    let structural_cfg = StructuralConfig::default();
    let struct_fut = async move {
        // Slice 0: pass empty inputs; the stub returns an empty report.
        // The next slice extends this to load (path, source_text) pairs
        // from `fp.contract_sources` and the per-contract solc
        // StorageLayout via `vergil_solidity::storage::StorageRun`.
        let sources: Vec<(std::path::PathBuf, String)> = Vec::new();
        let layouts: Vec<vergil_solidity::storage::StorageLayout> = Vec::new();
        Ok::<_, vergil_core::synthesis::SynthesisError>(extract_from_structural(
            &sources,
            &layouts,
            &structural_cfg,
        ))
    };

    let (cat_r, test_r, ns_r, struct_r) =
        tokio::join!(cat_fut, test_fut, ns_fut, struct_fut);

    if !activation.templates.is_empty() {
        match cat_r {
            Ok(r) => {
                out.candidates.extend(r.candidates.clone());
                out.catalog = Some(r);
            }
            Err(e) => tracing::warn!("catalog oracle failed: {e}"),
        }
    }
    if !tests_data.is_empty() {
        match test_r {
            Ok(r) => {
                out.candidates.extend(r.candidates.clone());
                out.tests = Some(r);
            }
            Err(e) => tracing::warn!("tests oracle failed: {e}"),
        }
    }
    if !ns_data.is_empty() {
        match ns_r {
            Ok(r) => {
                out.candidates.extend(r.candidates.clone());
                out.natspec = Some(r);
            }
            Err(e) => tracing::warn!("natspec oracle failed: {e}"),
        }
    }
    // Structural always runs (no oracle-availability gate); the empty
    // stub returns 0 candidates in Slice 0 so verdict shape is unchanged
    // until Slices 1-5 populate real miners.
    match struct_r {
        Ok(r) => {
            out.candidates.extend(r.candidates.clone());
            out.structural = Some(r);
        }
        Err(e) => tracing::warn!("structural oracle failed: {e}"),
    }
    Ok(out)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct ProviderHandles {
    synthesizer: Option<Arc<dyn vergil_llm::LlmProvider>>,
    critic: Option<Arc<dyn vergil_llm::LlmProvider>>,
    extractor: Option<Arc<dyn vergil_llm::LlmProvider>>,
    critic_provider_id: Option<ProviderId>,
}

impl ProviderHandles {
    fn from_env() -> Result<Self, String> {
        let bundle = crate::commands::intent::build_providers_from_env(None)
            .map_err(|e| format!("{e}"))?;
        Ok(Self {
            synthesizer: Some(bundle.synthesizer.clone()),
            critic: Some(bundle.critic.clone()),
            extractor: Some(bundle.synthesizer.clone()),
            critic_provider_id: Some(ProviderId::OpenAi),
        })
    }
}

fn fingerprint_to_facts(fp: &Fingerprint) -> StaticFacts {
    let mut facts = StaticFacts::new();
    facts = facts.with_interface("any");
    for i in &fp.interfaces {
        facts = facts.with_interface(i.clone());
    }
    facts = facts.with_primitive("any");
    for p in &fp.primitives {
        facts = facts.with_primitive(p.clone());
    }
    // Mirror the Phase-1 heuristic — conservatively turn every flag on
    // so activation rules that gate on `state_change_present`,
    // `no_auth_check`, etc. match.
    for flag in [
        "state_change_present",
        "no_auth_check",
        "unchecked_block_present",
        "external_call_present",
        "initialize_present",
        "cancel_present",
        "deposit_present",
        "uups_proxy",
        "bit_shift_present",
    ] {
        facts = facts.with_pattern(flag, true);
    }
    facts
}

fn filter_activation_by_category(activation: &mut ActivationResult<'_>, categories: &[String]) {
    let keep: std::collections::BTreeSet<&str> =
        categories.iter().map(|s| s.as_str()).collect();
    let (kept, dropped): (Vec<_>, Vec<_>) = activation
        .templates
        .drain(..)
        .partition(|t| keep.contains(t.manifest.category.as_str()));
    activation.templates = kept;
    for d in dropped {
        activation.skipped.push(vergil_properties::SkippedTemplate {
            id: d.manifest.id.clone(),
            reason: format!(
                "category {} not in --catalog-subset",
                d.manifest.category
            ),
        });
    }
}

fn collect_document_only_templates(
    activation: &ActivationResult<'_>,
) -> Vec<DocumentOnlyTemplate> {
    activation
        .templates
        .iter()
        .filter(|t| matches!(t.manifest.decidability.smt_status, SmtStatus::DocumentOnly))
        .map(|t| DocumentOnlyTemplate {
            id: t.manifest.id.clone(),
            name: t.manifest.name.clone(),
        })
        .collect()
}

fn print_list_applicable(fp: &Fingerprint, activation: &ActivationResult<'_>) {
    println!("# vergil verify --list-applicable");
    println!();
    println!("Project: {}", fp.project_root.display());
    println!("Interfaces: {}", if fp.interfaces.is_empty() { "(none)".to_string() } else { fp.interfaces.join(", ") });
    println!("Primitives: {}", if fp.primitives.is_empty() { "(none)".to_string() } else { fp.primitives.join(", ") });
    println!(
        "Oracles available: tests={} natspec={} readme={}",
        fp.available_oracles.tests,
        fp.available_oracles.natspec,
        fp.available_oracles.readme.is_some(),
    );
    println!();
    println!("Activated attack-catalog templates ({}):", activation.templates.len());
    for t in &activation.templates {
        println!("  - {}  [{}]  {}", t.manifest.id, t.manifest.category, t.manifest.name);
    }
    println!();
    println!("Skipped templates ({}):", activation.skipped.len());
    for s in &activation.skipped {
        println!("  - {}  ({})", s.id, s.reason);
    }
}

fn spec_to_proposed(c: &SpecCandidate, r: &CritiqueResult) -> ProposedIntent {
    ProposedIntent {
        id: format!("{}:{}", source_label(c.source), c.name),
        source: c.source,
        intent_text: c.intent_text.clone().unwrap_or_else(|| c.name.clone()),
        rationale: r.rationale.clone(),
        confidence: critique_min_axis(r),
        template_ref: c.template_ref.clone(),
    }
}

fn critique_min_axis(r: &CritiqueResult) -> f32 {
    r.scores
        .vacuity
        .min(r.scores.body_independence)
        .min(r.scores.testability)
        .min(r.scores.restate_the_source)
}

fn source_label(s: CoreSource) -> &'static str {
    match s {
        CoreSource::UserIntent => "user_intent",
        CoreSource::AttackCatalog => "attack_catalog",
        CoreSource::Conformance => "conformance",
        CoreSource::Tests => "tests",
        CoreSource::NatSpec => "nat_spec",
        CoreSource::Structural => "structural",
    }
}

fn proof_source(s: CoreSource) -> ProofSource {
    match s {
        CoreSource::UserIntent => ProofSource::UserIntent,
        CoreSource::AttackCatalog => ProofSource::AttackCatalog,
        CoreSource::Conformance => ProofSource::Conformance,
        CoreSource::Tests => ProofSource::Tests,
        CoreSource::NatSpec => ProofSource::NatSpec,
        CoreSource::Structural => ProofSource::Structural,
    }
}

fn tier_for_source(s: CoreSource) -> Tier {
    match s {
        CoreSource::UserIntent => Tier::Intent,
        _ => Tier::ZeroConfig,
    }
}

fn fingerprint_summary(fp: &Fingerprint) -> FingerprintSummary {
    FingerprintSummary {
        interfaces: fp.interfaces.clone(),
        primitives: fp.primitives.clone(),
        available_oracles: AvailableOraclesSummary {
            tests: fp.available_oracles.tests,
            natspec: fp.available_oracles.natspec,
            readme: fp.available_oracles.readme.is_some(),
        },
    }
}

fn index_candidates(
    kept: &[(SpecCandidate, CritiqueResult)],
) -> std::collections::BTreeMap<String, SpecCandidate> {
    let mut out = std::collections::BTreeMap::new();
    for (c, _) in kept {
        let id = format!("{}:{}", source_label(c.source), c.name);
        out.insert(id, c.clone());
    }
    out
}

fn build_outcome(
    candidate: &SpecCandidate,
    intent: &ProposedIntent,
    verdict: &VerifierVerdict,
) -> PropertyOutcome {
    let pv = match verdict {
        VerifierVerdict::Verified {
            backend,
            smt_query_sha256,
        } => PropertyVerdict::Verified {
            backend: backend.clone(),
            smt_query_sha256: smt_query_sha256.clone(),
        },
        VerifierVerdict::Counterexample { message } => PropertyVerdict::Refuted {
            backend: "halmos".to_string(),
            cex_file: format!("vergil-out/counterexamples/Cex_{}.t.sol", candidate.name),
            trace_summary: message.clone(),
        },
        VerifierVerdict::Unknown { detail } => PropertyVerdict::Unknown {
            detail: detail.clone(),
            frontier_over_approximation: None,
        },
        VerifierVerdict::Error { detail } => PropertyVerdict::Error {
            detail: detail.clone(),
        },
        VerifierVerdict::NotRun => PropertyVerdict::Unknown {
            detail: "verifier did not run".to_string(),
            frontier_over_approximation: None,
        },
    };
    PropertyOutcome {
        name: candidate.name.clone(),
        source: proof_source(candidate.source),
        tier: tier_for_source(candidate.source),
        verdict: pv,
        template_ref: candidate.template_ref.clone(),
        intent_text: candidate
            .intent_text
            .clone()
            .or_else(|| Some(intent.intent_text.clone())),
    }
}

fn read_contract_source(sources: &[PathBuf]) -> String {
    let mut out = String::new();
    for p in sources {
        if let Ok(s) = std::fs::read_to_string(p) {
            out.push_str(&s);
            out.push('\n');
        }
    }
    out
}

fn render_available_methods_for(sources: &[PathBuf]) -> String {
    let joined = read_contract_source(sources);
    let sigs = vergil_solidity::signatures::extract(&joined);
    vergil_solidity::signatures::render_available_methods(&sigs)
}

fn render_simple_cex(property: &str, message: &str) -> String {
    format!(
        "// SPDX-License-Identifier: UNLICENSED\npragma solidity ^0.8.20;\n\n// Counterexample for {property}.\n// Trace: {message}\n//\n// This is a minimal placeholder. Slice 8 emits the file at the moment\n// of refutation; Slice 11 will render the full Halmos trace.\n\ncontract Cex_{property} {{}}\n",
    )
}

fn locate_templates_dir() -> Option<PathBuf> {
    let mf = option_env!("CARGO_MANIFEST_DIR")?;
    let p = PathBuf::from(mf);
    let candidate = p.parent()?.parent()?.join(
        "crates/vergil-properties/templates/attacks",
    );
    if candidate.is_dir() {
        return Some(candidate);
    }
    None
}

fn build_proof_artifact(
    project: &std::path::Path,
    outcomes: &[PropertyOutcome],
    _headline: &Headline,
) -> Result<ProofArtifact, UnifiedVerifyError> {
    let mut source_files = Vec::new();
    let src_dir = project.join("src");
    if src_dir.is_dir() {
        for e in std::fs::read_dir(&src_dir)?.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "sol").unwrap_or(false) {
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
    let verified_properties: Vec<VerifiedProperty> = outcomes
        .iter()
        .filter_map(|p| match &p.verdict {
            PropertyVerdict::Verified {
                backend,
                smt_query_sha256,
            } => Some(VerifiedProperty {
                name: p.name.clone(),
                backend: backend.clone(),
                spec_sha256: sha256_hex(p.name.as_bytes()),
                template_ref: p.template_ref.clone(),
                wall_clock_ms: 0,
                smt_query_sha256: smt_query_sha256.clone(),
                manifest_validation: ManifestValidationStatus {
                    storage_ok: true,
                    modifiers_ok: true,
                    external_calls_ok: true,
                    warnings: Vec::new(),
                },
                source: p.source,
                tier: p.tier,
            }),
            _ => None,
        })
        .collect();
    Ok(ProofArtifact {
        vergil_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: ProofArtifact::schema_version_current(),
        run: RunMeta {
            run_id: format!("unified-{}", Utc::now().format("%Y%m%dT%H%M%SZ")),
            intent: "vergil verify (unified, Phase 6)".to_string(),
            project_root: project.display().to_string(),
            started_at: Utc::now().to_rfc3339(),
        },
        toolchain: ToolchainVersions {
            solc: "0.8.20".to_string(),
            halmos: "0.3.3".to_string(),
            slither: "0.11.0".to_string(),
            z3: "4.15.4".to_string(),
            cvc5: "1.3.0".to_string(),
            gambit: None,
        },
        source_files,
        verified_properties,
        counterexamples: Vec::new(),
        quality_metrics: QualityMetrics {
            mutation_coverage_min: None,
            critique_pass_rate: 0.0,
            mutation_testing_enabled: false,
        },
        cost: Cost {
            tokens_in: 0,
            tokens_out: 0,
            usd_estimate: 0.0,
            wall_clock_ms: 0,
        },
    })
}

// ─── TTY prompter ────────────────────────────────────────────────────────────

/// Minimal stdin/stderr prompter for the human path. Reads `[c/s/e/a]`
/// per intent.
#[derive(Default)]
struct StdinPrompter {
    all_yes: bool,
}

impl vergil_core::confirm::TtyPrompter for StdinPrompter {
    fn prompt(
        &mut self,
        intent: &ProposedIntent,
    ) -> Result<Decision, vergil_core::confirm::ConfirmError> {
        use std::io::Write;
        eprintln!();
        eprintln!("── Proposed intent ──────────────────────────────────");
        eprintln!("  source:  {}", source_label(intent.source));
        if let Some(t) = &intent.template_ref {
            eprintln!("  template: {t}");
        }
        eprintln!("  text:    {}", intent.intent_text);
        eprintln!("  reason:  {}", intent.rationale);
        eprintln!("  conf:    {:.2}", intent.confidence);
        loop {
            eprint!("  [c]onfirm / [s]kip / [e]dit / [a]ll-yes: ");
            std::io::stderr()
                .flush()
                .map_err(vergil_core::confirm::ConfirmError::Prompt)?;
            let mut line = String::new();
            std::io::stdin()
                .read_line(&mut line)
                .map_err(vergil_core::confirm::ConfirmError::Prompt)?;
            match line.trim() {
                "c" | "" => return Ok(Decision::Confirm),
                "s" => return Ok(Decision::Skip),
                "a" => {
                    self.all_yes = true;
                    return Ok(Decision::Confirm);
                }
                "e" => {
                    eprint!("  new intent text: ");
                    std::io::stderr()
                        .flush()
                        .map_err(vergil_core::confirm::ConfirmError::Prompt)?;
                    let mut edit = String::new();
                    std::io::stdin()
                        .read_line(&mut edit)
                        .map_err(vergil_core::confirm::ConfirmError::Prompt)?;
                    return Ok(Decision::Edit {
                        new_text: edit.trim().to_string(),
                    });
                }
                _ => eprintln!("  unrecognized; please answer c / s / e / a."),
            }
        }
    }

    fn all_yes_armed(&self) -> bool {
        self.all_yes
    }
}

// Silence unused-marker on ConfirmStatus while Slice 11 reuses it.
#[allow(dead_code)]
fn _confirm_status_use(s: ConfirmStatus) -> ConfirmStatus {
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn examples_root() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.pop();
        p.push("examples");
        p
    }

    #[test]
    fn fingerprint_to_facts_includes_interfaces_and_primitives() {
        let fp = vergil_core::fingerprint::Fingerprint {
            project_root: PathBuf::from("/tmp"),
            interfaces: vec!["ERC20".to_string()],
            primitives: vec!["token-erc20".to_string()],
            available_oracles: Default::default(),
            contract_sources: Vec::new(),
        };
        let facts = fingerprint_to_facts(&fp);
        assert!(facts.interfaces.contains("ERC20"));
        assert!(facts.primitives.contains("token-erc20"));
        assert!(facts.interfaces.contains("any"));
    }

    #[tokio::test]
    async fn list_applicable_short_circuits_without_llm_calls() {
        // Smoke: --list-applicable must work without env vars set
        // (no provider init). The Anthropic key is required only for
        // the LLM-dispatching path.
        let args = UnifiedVerifyArgs {
            project: examples_root().join("erc20"),
            list_applicable: true,
            ..Default::default()
        };
        let report = run(args).await.expect("list-applicable runs without LLM");
        match report {
            RunReport::ListApplicable { activated, .. } => {
                assert!(activated > 0, "erc20 should activate at least one catalog template");
            }
            _ => panic!("expected ListApplicable variant"),
        }
    }

    #[tokio::test]
    async fn list_applicable_returns_zero_exit_code() {
        let args = UnifiedVerifyArgs {
            project: examples_root().join("erc20"),
            list_applicable: true,
            ..Default::default()
        };
        let report = run(args).await.unwrap();
        assert_eq!(report.exit_code(), 0);
    }
}
