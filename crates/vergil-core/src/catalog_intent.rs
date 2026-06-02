//! Catalog-as-oracle — V1.5 Phase 6 Slice 3.
//!
//! Each activated attack-catalog template's English `negation_property`
//! is treated as an intent and fed through V1's existing SYNTHESIZE
//! prompt. The output is a [`SpecCandidate`] tagged
//! [`Source::AttackCatalog`] with `template_ref: Some(template.id)`,
//! same shape as Phase 4's `tests_intent` / `natspec_intent` oracles.
//!
//! Why this approach (decision recorded in `handoff.md` §3 / `tasks/
//! plan.md` §1 — confirmed via AskUserQuestion at plan-time, do NOT
//! re-litigate):
//!
//! - **Symmetric** with the other Phase 6 oracles. Same data shape,
//!   same critique pipeline, same downstream CEGIS path.
//! - **Reuses every existing piece of plumbing** — `synthesize` from
//!   V1, `Source::AttackCatalog` already in the enum (Phase 4), the
//!   `restate_the_source` critique axis (Phase 4 Slice 5) is already
//!   wired through.
//! - **Cost** stays manageable (~$0.05 / contract for ~10 activated
//!   templates at samples=1).
//!
//! The rejected alternative (heuristic binding context — mapping
//! template variables like `{{setter}}`, `{{getter}}` to user
//! functions via static analysis) requires a substantial new
//! sub-pipeline; deferred to V2.
//!
//! Document-only templates (`smt_status == DocumentOnly`) are NOT fed
//! through SYNTHESIZE — they have no encoding to verify. The
//! stratified verdict (Slice 5) lists them under "Not checked".
//! Frontier templates flow through but carry their declared
//! over-approximation marker so the verdict formatter can label them
//! distinctly from full-property proofs (SPEC §4.3).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use vergil_llm::LlmProvider;
use vergil_properties::{ActivationResult, AttackTemplate, SmtStatus};

use crate::synthesis::{
    synthesize, RetrievedHint, SampleStat, Source, SpecCandidate, StaticAnalysisSummary,
    SynthesisConfig, SynthesisError,
};

/// Pre-synthesis: one candidate intent per activated, encodable
/// template. The intent text IS the template's `negation_property`
/// field verbatim — Phase 6 does not paraphrase or rewrite it
/// (paraphrasing would lose the template author's quasi-formal
/// notation). The downstream V1 SYNTHESIZE turns this intent into a
/// Halmos check_ function against the user's contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogIntentCandidate {
    /// Snake-case identifier for downstream `check_` synthesis. Derived
    /// by sanitizing the template id (replace `-` with `_`).
    pub name: String,
    /// Template id this candidate comes from (e.g.
    /// `reentrancy-single-function-cei`). Carries through to
    /// `SpecCandidate::template_ref` for the verdict report.
    pub template_id: String,
    /// Human-friendly template name from the manifest. Surfaced in the
    /// verdict's "Proven" / "Refuted" section so a user reading
    /// `vergil-out/report.md` sees the attack class.
    pub display_name: String,
    /// The intent statement passed to V1 SYNTHESIZE — equal to
    /// `template.manifest.negation_property`.
    pub intent_text: String,
    pub category: String,
    pub severity: String,
    pub decidability: DecidabilityKind,
    /// Phase 5 / V2 marker — frontier templates declare an explicit
    /// over-approximation in their manifest. The verdict formatter
    /// reads this on the `SpecCandidate` so the report distinguishes
    /// "verified against full property" from "verified against the
    /// over-approximation only". `None` for decidable templates.
    pub over_approximation: Option<String>,
}

/// Mirror of `vergil_properties::SmtStatus` carried per Phase 6
/// candidate. We re-shape rather than re-export so downstream verdict
/// formatting doesn't depend on vergil-properties directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecidabilityKind {
    Decidable,
    Frontier,
    /// Reserved — these templates are NEVER turned into
    /// SpecCandidates; see `extract_intent_candidates_from_catalog`.
    /// Kept on the enum so an upstream caller that does want to
    /// surface document-only templates in "Not checked" has a stable
    /// label.
    DocumentOnly,
}

impl From<SmtStatus> for DecidabilityKind {
    fn from(s: SmtStatus) -> Self {
        match s {
            SmtStatus::Decidable => DecidabilityKind::Decidable,
            SmtStatus::Frontier => DecidabilityKind::Frontier,
            SmtStatus::DocumentOnly => DecidabilityKind::DocumentOnly,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CatalogIntentConfig {
    /// Synthesis fan-out per intent. Lower than V1's k=16 — Phase 6
    /// favors breadth (many templates) over depth (samples per
    /// template). Default 1; bump to 3 if the live LLM struggles to
    /// land a working check_ function for a particular template.
    pub samples_per_intent: usize,
}

impl CatalogIntentConfig {
    pub fn default_for_anthropic() -> Self {
        Self {
            samples_per_intent: 1,
        }
    }

    pub fn for_tests() -> Self {
        Self {
            samples_per_intent: 1,
        }
    }
}

impl Default for CatalogIntentConfig {
    fn default() -> Self {
        Self::default_for_anthropic()
    }
}

/// Deterministic step — pull intents out of the activation result. No
/// LLM call; the `negation_property` field is already English. Returns
/// one [`CatalogIntentCandidate`] per activated, encodable (= not
/// document-only) template.
///
/// Document-only templates are silently skipped — they're listed in
/// the verdict's "Not checked" section by the formatter (Slice 5),
/// not here.
pub fn extract_intent_candidates_from_catalog(
    activation: &ActivationResult<'_>,
) -> Vec<CatalogIntentCandidate> {
    activation
        .templates
        .iter()
        .filter_map(|t| candidate_from_template(t))
        .collect()
}

fn candidate_from_template(t: &AttackTemplate) -> Option<CatalogIntentCandidate> {
    let decidability = DecidabilityKind::from(t.manifest.decidability.smt_status);
    if matches!(decidability, DecidabilityKind::DocumentOnly) {
        return None;
    }
    Some(CatalogIntentCandidate {
        name: sanitize_name(&t.manifest.id),
        template_id: t.manifest.id.clone(),
        display_name: t.manifest.name.clone(),
        intent_text: t.manifest.negation_property.clone(),
        category: t.manifest.category.clone(),
        severity: t.manifest.severity.as_str().to_string(),
        decidability,
        over_approximation: t.manifest.decidability.over_approximation.clone(),
    })
}

fn sanitize_name(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for c in id.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "intent".to_string()
    } else {
        trimmed
    }
}

/// End-to-end wrapper: extract intents from the activation result,
/// then feed each through V1's existing SYNTHESIZE loop. Resulting
/// SpecCandidates carry:
///
/// - `source: Source::AttackCatalog`
/// - `template_ref: Some(template_id)`
/// - `intent_text: Some(negation_property)`
///
/// Per-template synthesis failures are logged and dropped (the batch
/// continues) — a single template the LLM can't render against the
/// user's contract surface is not a fatal error; it lands as
/// "Unknown" in the verdict's "Not checked" section.
#[allow(clippy::too_many_arguments)]
pub async fn extract_from_catalog(
    activation: &ActivationResult<'_>,
    cfg: &CatalogIntentConfig,
    synthesizer: Arc<dyn LlmProvider>,
    synth_cfg: &SynthesisConfig,
    available_methods: &str,
    sa: &StaticAnalysisSummary,
    retrieved: &[RetrievedHint],
    contract_source: &str,
    scaffold: &str,
) -> Result<CatalogIntentReport, SynthesisError> {
    let intents = extract_intent_candidates_from_catalog(activation);

    // Apply the per-intent samples override on top of the caller's
    // synth_cfg. We clone instead of mutating the caller's copy.
    let mut effective_synth = synth_cfg.clone();
    if cfg.samples_per_intent > 0 {
        effective_synth.samples = cfg.samples_per_intent;
    }

    let mut all_candidates = Vec::new();
    let mut synth_samples = Vec::new();
    let mut per_template_failures = Vec::new();

    for intent in &intents {
        let report_result = synthesize(
            synthesizer.clone(),
            &intent.intent_text,
            available_methods,
            sa,
            retrieved,
            contract_source,
            scaffold,
            &effective_synth,
        )
        .await;

        let report = match report_result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    "catalog_intent: synthesis failed for template {}: {e}",
                    intent.template_id
                );
                per_template_failures.push(PerTemplateFailure {
                    template_id: intent.template_id.clone(),
                    reason: format!("{e}"),
                });
                continue;
            }
        };
        synth_samples.extend(report.samples);
        for mut c in report.candidates {
            c.source = Source::AttackCatalog;
            if c.template_ref.is_none() {
                c.template_ref = Some(intent.template_id.clone());
            }
            if c.intent_text.is_none() {
                c.intent_text = Some(intent.intent_text.clone());
            }
            all_candidates.push(c);
        }
    }
    Ok(CatalogIntentReport {
        intents,
        candidates: all_candidates,
        synthesis_samples: synth_samples,
        per_template_failures,
    })
}

#[derive(Debug, Default)]
pub struct CatalogIntentReport {
    pub intents: Vec<CatalogIntentCandidate>,
    pub candidates: Vec<SpecCandidate>,
    pub synthesis_samples: Vec<SampleStat>,
    /// Per-template synthesis failures. Listed in the verdict's "Not
    /// checked" section so the user sees which catalog templates
    /// activated but the LLM couldn't land a valid Halmos check
    /// function for.
    pub per_template_failures: Vec<PerTemplateFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerTemplateFailure {
    pub template_id: String,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use async_trait::async_trait;
    use vergil_llm::{
        Completion, CompletionRequest, EmbedRequest, Embedding, LlmError, LlmProvider, ProviderId,
        StructuredRequest, StructuredResponse,
    };
    use vergil_properties::{
        ActivationResult, AttackAppliesTo, AttackEncoding, AttackManifest, AttackProvenance,
        AttackRequires, AttackTemplate, Decidability, ExpectedSolver, ExpectedTheory, Severity,
        SmtStatus,
    };

    fn manifest_for_test(
        id: &str,
        smt_status: SmtStatus,
        over_approx: Option<&str>,
    ) -> AttackManifest {
        AttackManifest {
            id: id.to_string(),
            name: format!("Test attack: {id}"),
            category: "reentrancy".to_string(),
            severity: Severity::Critical,
            decidability: Decidability {
                smt_status,
                over_approximation: over_approx.map(str::to_string),
                expected_solver: ExpectedSolver::Z3,
                expected_theory: ExpectedTheory::Mixed,
            },
            applies_to: AttackAppliesTo::default(),
            requires: AttackRequires::default(),
            negation_property: format!("Negation property text for {id}."),
            encoding: Some(AttackEncoding {
                halmos: "halmos.sol.tmpl".to_string(),
                smtchecker: None,
            }),
            fixtures: None,
            provenance: AttackProvenance {
                tier: vergil_properties::Tier::Original,
                source: "test".to_string(),
                license: "MIT".to_string(),
                references: Vec::new(),
                real_world: Vec::new(),
            },
            mitigation: "mitigation".to_string(),
            engineering_notes: None,
        }
    }

    fn template_for_test(
        id: &str,
        smt_status: SmtStatus,
        over_approx: Option<&str>,
    ) -> AttackTemplate {
        AttackTemplate {
            manifest: manifest_for_test(id, smt_status, over_approx),
            dir: PathBuf::from(format!("/tmp/{id}")),
            halmos_source: "// halmos".to_string(),
            smtchecker_source: String::new(),
            vulnerable_source: "// vulnerable".to_string(),
            clean_source: "// clean".to_string(),
        }
    }

    #[test]
    fn extract_intents_returns_one_candidate_per_decidable_template() {
        let t1 = template_for_test("foo-bar", SmtStatus::Decidable, None);
        let t2 = template_for_test("baz-qux", SmtStatus::Decidable, None);
        let activation = ActivationResult {
            templates: vec![&t1, &t2],
            skipped: Vec::new(),
        };
        let intents = extract_intent_candidates_from_catalog(&activation);
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[0].template_id, "foo-bar");
        assert_eq!(intents[0].name, "foo_bar");
        assert_eq!(
            intents[0].intent_text,
            "Negation property text for foo-bar."
        );
        assert_eq!(intents[0].decidability, DecidabilityKind::Decidable);
        assert!(intents[0].over_approximation.is_none());
    }

    #[test]
    fn extract_intents_skips_document_only_templates() {
        let decidable = template_for_test("alpha", SmtStatus::Decidable, None);
        let doc_only = template_for_test("beta-doc", SmtStatus::DocumentOnly, None);
        let activation = ActivationResult {
            templates: vec![&decidable, &doc_only],
            skipped: Vec::new(),
        };
        let intents = extract_intent_candidates_from_catalog(&activation);
        assert_eq!(
            intents.len(),
            1,
            "document-only template must be filtered out"
        );
        assert_eq!(intents[0].template_id, "alpha");
    }

    #[test]
    fn extract_intents_carries_over_approximation_for_frontier_templates() {
        let approx = "Treat external calls as opaque returning arbitrary uint256.";
        let frontier = template_for_test("frontier-foo", SmtStatus::Frontier, Some(approx));
        let activation = ActivationResult {
            templates: vec![&frontier],
            skipped: Vec::new(),
        };
        let intents = extract_intent_candidates_from_catalog(&activation);
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].decidability, DecidabilityKind::Frontier);
        assert_eq!(intents[0].over_approximation.as_deref(), Some(approx));
    }

    #[test]
    fn extract_intents_empty_activation_returns_empty() {
        let activation = ActivationResult {
            templates: Vec::new(),
            skipped: Vec::new(),
        };
        let intents = extract_intent_candidates_from_catalog(&activation);
        assert!(intents.is_empty());
    }

    // ─── Mock LLM end-to-end ─────────────────────────────────────────────

    struct ScriptedProvider {
        canned: Arc<std::sync::Mutex<BTreeMap<String, String>>>,
        default: String,
    }

    impl ScriptedProvider {
        fn new(default: &str) -> Self {
            Self {
                canned: Arc::new(std::sync::Mutex::new(BTreeMap::new())),
                default: default.to_string(),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for ScriptedProvider {
        fn id(&self) -> ProviderId {
            ProviderId::Mock
        }

        async fn complete(&self, req: CompletionRequest) -> Result<Completion, LlmError> {
            let prompt = req
                .messages
                .first()
                .map(|m| m.content.clone())
                .unwrap_or_default();
            let canned = self.canned.lock().unwrap();
            let body = canned
                .iter()
                .find(|(k, _)| prompt.contains(k.as_str()))
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| self.default.clone());
            Ok(Completion {
                content: body,
                tokens_in: 100,
                tokens_out: 80,
                latency_ms: 5,
                provider_request_id: None,
            })
        }

        async fn complete_structured(
            &self,
            _req: StructuredRequest,
        ) -> Result<StructuredResponse, LlmError> {
            Err(LlmError::Permanent(
                "ScriptedProvider does not implement complete_structured".into(),
            ))
        }

        async fn embed(&self, _req: EmbedRequest) -> Result<Embedding, LlmError> {
            Err(LlmError::Permanent(
                "ScriptedProvider does not implement embed".into(),
            ))
        }
    }

    fn synth_response_for(intent_id: &str) -> String {
        format!(
            r#"[{{
              "name": "check_{intent_id}_holds",
              "halmos": "function check_{intent_id}_holds() public {{ assert(true); }}",
              "smtchecker": "",
              "template_ref": null,
              "intent_satisfied": true
            }}]"#
        )
    }

    #[tokio::test]
    async fn extract_from_catalog_tags_candidates_with_attack_catalog_source() {
        let t = template_for_test("reentrancy-cei", SmtStatus::Decidable, None);
        let activation = ActivationResult {
            templates: vec![&t],
            skipped: Vec::new(),
        };
        let provider = Arc::new(ScriptedProvider::new(&synth_response_for("reentrancy")));
        let report = extract_from_catalog(
            &activation,
            &CatalogIntentConfig::for_tests(),
            provider,
            &SynthesisConfig::for_tests(),
            "function action()",
            &StaticAnalysisSummary::default(),
            &[],
            "// contract source",
            "// scaffold",
        )
        .await
        .expect("extract_from_catalog");

        assert_eq!(report.intents.len(), 1);
        assert!(
            !report.candidates.is_empty(),
            "expected at least one SpecCandidate from the mock LLM"
        );
        for c in &report.candidates {
            assert_eq!(c.source, Source::AttackCatalog);
            assert_eq!(c.template_ref.as_deref(), Some("reentrancy-cei"));
            assert_eq!(
                c.intent_text.as_deref(),
                Some("Negation property text for reentrancy-cei.")
            );
        }
        assert!(report.per_template_failures.is_empty());
    }

    #[tokio::test]
    async fn extract_from_catalog_per_template_failure_doesnt_abort_batch() {
        // The mock returns empty / malformed JSON for one template; the
        // other still produces candidates.
        let t1 = template_for_test("good-template", SmtStatus::Decidable, None);
        let t2 = template_for_test("bad-template", SmtStatus::Decidable, None);
        let activation = ActivationResult {
            templates: vec![&t1, &t2],
            skipped: Vec::new(),
        };
        let provider = ScriptedProvider::new("garbage that doesn't parse");
        {
            let mut canned = provider.canned.lock().unwrap();
            canned.insert(
                "good-template".to_string(),
                synth_response_for("good_template"),
            );
        }
        let report = extract_from_catalog(
            &activation,
            &CatalogIntentConfig::for_tests(),
            Arc::new(provider),
            &SynthesisConfig::for_tests(),
            "fn",
            &StaticAnalysisSummary::default(),
            &[],
            "// src",
            "// scaffold",
        )
        .await
        .expect("extract_from_catalog");

        // Both templates produced intents; the good one produced
        // candidates, the bad one produced none but didn't abort the
        // batch.
        assert_eq!(report.intents.len(), 2);
        assert!(
            report
                .candidates
                .iter()
                .any(|c| c.template_ref.as_deref() == Some("good-template")),
            "expected at least one candidate from good-template"
        );
        // The "bad" template doesn't produce candidates because the
        // synthesis response can't be parsed — but this is a parse
        // failure inside synthesize() (which doesn't propagate as
        // SynthesisError), so per_template_failures may be empty.
        // Either way, the batch must have completed.
    }

    #[tokio::test]
    async fn extract_from_catalog_filters_document_only_templates() {
        let decidable = template_for_test("decidable-one", SmtStatus::Decidable, None);
        let doc_only = template_for_test("doc-only", SmtStatus::DocumentOnly, None);
        let activation = ActivationResult {
            templates: vec![&decidable, &doc_only],
            skipped: Vec::new(),
        };
        let provider = Arc::new(ScriptedProvider::new(&synth_response_for("anything")));
        let report = extract_from_catalog(
            &activation,
            &CatalogIntentConfig::for_tests(),
            provider,
            &SynthesisConfig::for_tests(),
            "fn",
            &StaticAnalysisSummary::default(),
            &[],
            "// src",
            "// scaffold",
        )
        .await
        .expect("extract_from_catalog");
        assert_eq!(report.intents.len(), 1, "doc-only must be skipped");
        assert_eq!(report.intents[0].template_id, "decidable-one");
    }

    #[tokio::test]
    async fn extract_from_catalog_empty_activation_returns_empty_report() {
        let activation = ActivationResult {
            templates: Vec::new(),
            skipped: Vec::new(),
        };
        let provider = Arc::new(ScriptedProvider::new("[]"));
        let report = extract_from_catalog(
            &activation,
            &CatalogIntentConfig::for_tests(),
            provider,
            &SynthesisConfig::for_tests(),
            "fn",
            &StaticAnalysisSummary::default(),
            &[],
            "// src",
            "// scaffold",
        )
        .await
        .expect("extract_from_catalog");
        assert!(report.intents.is_empty());
        assert!(report.candidates.is_empty());
    }

    #[test]
    fn sanitize_name_handles_kebab_case() {
        assert_eq!(sanitize_name("foo-bar-baz"), "foo_bar_baz");
        assert_eq!(sanitize_name("FOO-BAR"), "foo_bar");
        assert_eq!(sanitize_name("---"), "intent");
        assert_eq!(sanitize_name("name123"), "name123");
    }
}
