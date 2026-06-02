//! Independent critique pass: a second LLM (ideally a different vendor)
//! scores each [`SpecCandidate`] for vacuity, body_independence, and
//! testability before the candidate ever reaches the solver.
//!
//! Cross-provider routing per SPEC §11.2 step 6: if synthesis used
//! Anthropic, critique uses OpenAI (and vice versa). When only one provider
//! is configured, fall back to same-provider critique at a different
//! temperature with the explicit CRITIQUE prompt — log a warning but
//! proceed (better one vacuity defense than zero).
//!
//! The verdict is hard: rejected candidates do not flow into the next
//! pipeline stage. Drop reasons (critique result + raw scores) are
//! persisted to `vergil-out/spec/critiques.json` for the report.

use std::collections::BTreeMap;
use std::sync::Arc;

use futures::future::join_all;
use serde::{Deserialize, Serialize};

use vergil_llm::prompts::CRITIQUE;
use vergil_llm::{LlmProvider, Message, ProviderId, Role, StructuredRequest};

use crate::synthesis::SpecCandidate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Accept,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CritiqueScores {
    pub vacuity: f32,
    pub body_independence: f32,
    pub testability: f32,
    /// Phase 4 §3.4a/b axis: rejects candidates that are just literal
    /// restatements of their source (test assertion or NatSpec doc
    /// comment). Defaults to 1.0 (no restatement to detect) so V1
    /// cassettes and user-intent candidates pass cleanly. Lower scores
    /// indicate the candidate didn't generalize beyond its source.
    #[serde(default = "one_f32")]
    pub restate_the_source: f32,
}

fn one_f32() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CritiqueResult {
    pub verdict: Verdict,
    pub scores: CritiqueScores,
    pub rationale: String,
}

#[derive(Debug, Clone)]
pub struct CritiqueConfig {
    pub model: String,
    pub max_tokens: u32,
    /// Critique-pass temperature. Slightly above zero so cross-provider
    /// disagreement surfaces, but low enough that the score is stable.
    pub temperature: f32,
    /// Minimum acceptable value on every axis. Defaults to 0.5 per SPEC.
    pub min_axis: f32,
}

impl CritiqueConfig {
    pub fn default_for_openai() -> Self {
        Self {
            model: "gpt-5.5".to_string(),
            max_tokens: 1024,
            temperature: 0.2,
            min_axis: 0.5,
        }
    }

    pub fn default_for_anthropic_fallback() -> Self {
        Self {
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 1024,
            temperature: 0.4,
            min_axis: 0.5,
        }
    }

    pub fn for_tests() -> Self {
        Self {
            model: "mock".to_string(),
            max_tokens: 512,
            temperature: 0.0,
            min_axis: 0.5,
        }
    }
}

pub struct Critic {
    critic: Arc<dyn LlmProvider>,
    synth_provider: ProviderId,
    cfg: CritiqueConfig,
}

impl Critic {
    pub fn new(
        critic: Arc<dyn LlmProvider>,
        synth_provider: ProviderId,
        cfg: CritiqueConfig,
    ) -> Self {
        if critic.id() == synth_provider {
            tracing::warn!(
                "Critic is using same provider as synthesizer ({:?}). Vacuity defense is weaker; configure both VERGIL_ANTHROPIC_API_KEY and VERGIL_OPENAI_API_KEY for cross-provider critique.",
                synth_provider
            );
        }
        Self {
            critic,
            synth_provider,
            cfg,
        }
    }

    pub fn provider_id(&self) -> ProviderId {
        self.critic.id()
    }

    pub fn is_cross_provider(&self) -> bool {
        self.critic.id() != self.synth_provider
    }

    /// Critique every candidate in parallel and return per-candidate results
    /// in input order. Candidates whose critique call errored receive a
    /// reject verdict with the error in the rationale.
    ///
    /// `description` is the property-specific statement when one is being
    /// targeted (kill criterion / batched runs); the critic uses it as the
    /// scoring anchor instead of the broader contract-level intent. Pass
    /// `None` for free-form `vergil verify --intent` runs.
    pub async fn critique_all(
        &self,
        candidates: &[SpecCandidate],
        intent: &str,
        description: Option<&str>,
    ) -> Vec<CritiqueResult> {
        let tasks: Vec<_> = candidates
            .iter()
            .map(|c| self.critique_one(c, intent, description))
            .collect();
        join_all(tasks).await
    }

    /// Filter the candidates to those whose verdict is Accept AND every axis
    /// is >= `cfg.min_axis`. Returns the surviving candidates paired with
    /// their critique result so the loop can persist + report both.
    pub fn filter_accepted(
        &self,
        candidates: Vec<SpecCandidate>,
        results: Vec<CritiqueResult>,
    ) -> FilterOutcome {
        assert_eq!(candidates.len(), results.len());
        let mut kept = Vec::new();
        let mut dropped = Vec::new();
        for (c, r) in candidates.into_iter().zip(results.into_iter()) {
            if matches!(r.verdict, Verdict::Accept) && self.passes_axes(&r.scores) {
                kept.push((c, r));
            } else {
                dropped.push((c, r));
            }
        }
        FilterOutcome { kept, dropped }
    }

    fn passes_axes(&self, s: &CritiqueScores) -> bool {
        s.vacuity >= self.cfg.min_axis
            && s.body_independence >= self.cfg.min_axis
            && s.testability >= self.cfg.min_axis
            && s.restate_the_source >= self.cfg.min_axis
    }

    async fn critique_one(
        &self,
        candidate: &SpecCandidate,
        intent: &str,
        description: Option<&str>,
    ) -> CritiqueResult {
        let prompt = match render(intent, candidate, description, self.cfg.min_axis) {
            Ok(p) => p,
            Err(e) => return reject_with(format!("prompt render failed: {e}"), candidate),
        };
        let req = StructuredRequest {
            model: self.cfg.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: prompt,
            }],
            system: Some(
                "You are an adversarial reviewer of formal specifications. Return ONLY the JSON object the user prompt's schema describes."
                    .to_string(),
            ),
            temperature: self.cfg.temperature,
            max_tokens: self.cfg.max_tokens,
            schema_name: "Critique".to_string(),
            schema: critique_schema(),
        };
        match self.critic.complete_structured(req).await {
            Ok(resp) => {
                let raw = resp.value.clone();
                match serde_json::from_value::<CritiqueResult>(resp.value) {
                    Ok(parsed) => parsed,
                    Err(e) => {
                        tracing::warn!(
                            "critique JSON shape error for {}: {e}; raw={:?}",
                            candidate.name,
                            raw
                        );
                        reject_with(format!("critique JSON shape: {e}"), candidate)
                    }
                }
            }
            Err(e) => {
                tracing::warn!("critique LLM error for {}: {e}", candidate.name);
                reject_with(format!("{e}"), candidate)
            }
        }
    }
}

fn render(
    intent: &str,
    candidate: &SpecCandidate,
    description: Option<&str>,
    min_axis: f32,
) -> Result<String, vergil_llm::prompts::PromptError> {
    let mut spec_source =
        String::with_capacity(candidate.halmos.len() + candidate.smtchecker.len() + 64);
    spec_source.push_str("// Halmos check_ function\n");
    spec_source.push_str(&candidate.halmos);
    if !candidate.smtchecker.is_empty() {
        spec_source.push_str("\n\n// SMTChecker fragment\n");
        spec_source.push_str(&candidate.smtchecker);
    }
    let desc = description
        .map(|d| d.trim())
        .filter(|d| !d.is_empty())
        .unwrap_or("(none — score against the broader intent)");
    let threshold = format!("{min_axis:.2}");
    let source_kind = source_label(candidate);
    let source_guidance = source_guidance(candidate);
    let derived_intent = candidate.intent_text.as_deref().unwrap_or("(none)");
    let mut vars: BTreeMap<&str, &str> = BTreeMap::new();
    vars.insert("intent", intent);
    vars.insert("description", desc);
    vars.insert("spec_source", &spec_source);
    vars.insert("min_axis", threshold.as_str());
    vars.insert("source_kind", source_kind);
    vars.insert("source_guidance", source_guidance);
    vars.insert("derived_intent", derived_intent);
    CRITIQUE.render(&vars)
}

/// Stable label for `candidate.source` used by the critique prompt to
/// pick the right scoring guidance for the restate-the-source axis.
fn source_label(candidate: &SpecCandidate) -> &'static str {
    use crate::synthesis::Source;
    match candidate.source {
        Source::UserIntent => "user_intent",
        Source::AttackCatalog => "attack_catalog",
        Source::Conformance => "conformance",
        Source::Tests => "tests",
        Source::NatSpec => "natspec",
        Source::Structural => "structural",
    }
}

/// Per-source scoring guidance for the new `restate_the_source` axis.
/// Phase 4 only differentiates `tests` and `natspec`; the others map to
/// "no check applies, score 1.0" so V1 user-intent candidates and
/// catalog-derived candidates pass through unchanged.
fn source_guidance(candidate: &SpecCandidate) -> &'static str {
    use crate::synthesis::Source;
    match candidate.source {
        Source::Tests => {
            "The candidate was derived from a Foundry / Halmos test function (Phase 4 §3.4a). \
             Score restate_the_source LOW when the derived intent — or the spec_source itself — is just a literal paraphrase of the test's `assertEq` / `assertTrue` / bare `assert(...)` calls without generalizing across symbolic inputs. \
             A candidate that asserts the SAME concrete check the test does (hardcoded amounts / addresses / single equality, no quantifier) belongs below the threshold. \
             A candidate that generalizes the test (e.g. \"for any amount, transfer preserves totalSupply\" from a single-amount test) belongs at or above the threshold."
        }
        Source::NatSpec => {
            "The candidate was derived from a NatSpec doc comment (Phase 4 §3.4b). \
             Score restate_the_source LOW when the derived intent — or the spec_source — is just a paraphrase of the doc comment's English without adding verifiable contract-state structure. \
             A candidate that turns `@notice transfers tokens` into `assert(transferHappened)` belongs below the threshold (the spec adds no real state-level check). \
             A candidate that turns the doc comment into a state-level invariant (e.g. \"transfer's accounting preserves totalSupply\") belongs at or above the threshold. \
             `@invariant` and `@custom:security` tags are by-design verbatim invariants; for those, the restate_the_source axis is satisfied as long as the spec_source actually exercises the named state."
        }
        _ => {
            "The candidate is not Phase-4-extracted; the restate-the-source axis does not apply. \
             Score restate_the_source = 1.0 unconditionally."
        }
    }
}

fn critique_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["verdict", "scores", "rationale"],
        "properties": {
            "verdict": { "type": "string", "enum": ["accept", "reject"] },
            "scores": {
                "type": "object",
                "required": [
                    "vacuity",
                    "body_independence",
                    "testability",
                    "restate_the_source"
                ],
                "properties": {
                    "vacuity": { "type": "number", "minimum": 0, "maximum": 1 },
                    "body_independence": { "type": "number", "minimum": 0, "maximum": 1 },
                    "testability": { "type": "number", "minimum": 0, "maximum": 1 },
                    "restate_the_source": { "type": "number", "minimum": 0, "maximum": 1 }
                }
            },
            "rationale": { "type": "string" }
        }
    })
}

fn reject_with(reason: String, candidate: &SpecCandidate) -> CritiqueResult {
    use crate::synthesis::Source;
    // For non-Phase-4 candidates, restate_the_source defaults to 1.0 so a
    // critique-pass error doesn't masquerade as a restate-the-source
    // rejection. For Phase 4 candidates it stays at 0.0 (same as the
    // other axes) — the error path is genuinely a reject across the board.
    let restate = if matches!(candidate.source, Source::Tests | Source::NatSpec) {
        0.0
    } else {
        1.0
    };
    CritiqueResult {
        verdict: Verdict::Reject,
        scores: CritiqueScores {
            vacuity: 0.0,
            body_independence: 0.0,
            testability: 0.0,
            restate_the_source: restate,
        },
        rationale: format!("critique-pass error: {reason}"),
    }
}

/// Wrapper that the loop can call to also map dropped-with-error counts —
/// used by the cost telemetry layer in Slice 13.
pub fn classify_critique_outcomes(results: &[CritiqueResult]) -> CritiqueOutcomeCounts {
    let mut out = CritiqueOutcomeCounts::default();
    for r in results {
        match r.verdict {
            Verdict::Accept => out.accepted += 1,
            Verdict::Reject => {
                if r.rationale.starts_with("critique-pass error") {
                    out.error += 1;
                } else {
                    out.rejected += 1;
                }
            }
        }
    }
    out
}

/// Result of [`Critic::filter_accepted`].
#[derive(Debug, Default)]
pub struct FilterOutcome {
    pub kept: Vec<(SpecCandidate, CritiqueResult)>,
    pub dropped: Vec<(SpecCandidate, CritiqueResult)>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CritiqueOutcomeCounts {
    pub accepted: usize,
    pub rejected: usize,
    pub error: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use vergil_llm::mock::MockProvider;

    fn sample_candidate() -> SpecCandidate {
        SpecCandidate {
            name: "check_transfer_preserves_supply".to_string(),
            halmos: "function check_transfer_preserves_supply(address to, uint256 amount) public { uint256 t0 = token.totalSupply(); try token.transfer(to, amount) {} catch {} assert(token.totalSupply() == t0); }".to_string(),
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: true,
            source: crate::synthesis::Source::UserIntent,
            intent_text: None,
        }
    }

    #[test]
    fn classify_counts_accept_reject_error() {
        let r = vec![
            CritiqueResult {
                verdict: Verdict::Accept,
                scores: CritiqueScores {
                    vacuity: 0.9,
                    body_independence: 0.8,
                    testability: 0.9,
                    restate_the_source: 1.0,
                },
                rationale: "ok".into(),
            },
            CritiqueResult {
                verdict: Verdict::Reject,
                scores: CritiqueScores {
                    vacuity: 0.1,
                    body_independence: 0.1,
                    testability: 0.1,
                    restate_the_source: 1.0,
                },
                rationale: "vacuous spec".into(),
            },
            CritiqueResult {
                verdict: Verdict::Reject,
                scores: CritiqueScores {
                    vacuity: 0.0,
                    body_independence: 0.0,
                    testability: 0.0,
                    restate_the_source: 1.0,
                },
                rationale: "critique-pass error: rate limit".into(),
            },
        ];
        let counts = classify_critique_outcomes(&r);
        assert_eq!(counts.accepted, 1);
        assert_eq!(counts.rejected, 1);
        assert_eq!(counts.error, 1);
    }

    #[test]
    fn passes_axes_uses_min_axis_threshold() {
        let mut cfg = CritiqueConfig::for_tests();
        cfg.min_axis = 0.5;
        let critic = Critic::new(
            Arc::new(MockProvider::new(PathBuf::from("/tmp/nope"))),
            ProviderId::Anthropic,
            cfg,
        );
        let ok = CritiqueScores {
            vacuity: 0.6,
            body_independence: 0.7,
            testability: 0.8,
            restate_the_source: 1.0,
        };
        let low_vac = CritiqueScores {
            vacuity: 0.4,
            body_independence: 0.9,
            testability: 0.9,
            restate_the_source: 1.0,
        };
        let low_restate = CritiqueScores {
            vacuity: 0.9,
            body_independence: 0.9,
            testability: 0.9,
            restate_the_source: 0.2,
        };
        assert!(critic.passes_axes(&ok));
        assert!(!critic.passes_axes(&low_vac));
        assert!(
            !critic.passes_axes(&low_restate),
            "restate_the_source axis should gate verdict"
        );
    }

    #[test]
    fn render_inlines_intent_and_spec() {
        let c = sample_candidate();
        let out = render("verify totalSupply preserved", &c, None, 0.4).unwrap();
        assert!(out.contains("verify totalSupply preserved"));
        assert!(out.contains("check_transfer_preserves_supply"));
        assert!(!out.contains("{{"));
        // min_axis threading
        assert!(out.contains("0.40"));
        // description block falls back to a non-empty default
        assert!(out.contains("(none — score against the broader intent)"));
    }

    #[test]
    fn render_passes_description_through() {
        let c = sample_candidate();
        let out = render(
            "ERC-20 conformance",
            &c,
            Some("totalSupply must not change across any transfer"),
            0.4,
        )
        .unwrap();
        assert!(out.contains("totalSupply must not change across any transfer"));
        assert!(!out.contains("(none — score against the broader intent)"));
    }

    #[test]
    fn cross_provider_flag_is_correct() {
        let critic = Critic::new(
            Arc::new(MockProvider::new(PathBuf::from("/tmp/nope"))),
            ProviderId::Anthropic,
            CritiqueConfig::for_tests(),
        );
        // MockProvider.id() == ProviderId::Mock, synth_provider == Anthropic
        assert!(critic.is_cross_provider());
        let critic = Critic::new(
            Arc::new(MockProvider::new(PathBuf::from("/tmp/nope"))),
            ProviderId::Mock,
            CritiqueConfig::for_tests(),
        );
        assert!(!critic.is_cross_provider());
    }

    // --- Phase 4 Slice 5: restate-the-source axis ---

    use crate::synthesis::Source;
    use vergil_llm::{request_sha, sha_hex, Message, Role, StructuredRequest};

    fn tests_derived_candidate(literal: bool) -> SpecCandidate {
        SpecCandidate {
            name: if literal {
                "check_alice_has_100".to_string()
            } else {
                "check_transfer_preserves_supply".to_string()
            },
            halmos: if literal {
                "function check_alice_has_100() public { assert(token.balanceOf(0xA11ce) == 100); }".to_string()
            } else {
                "function check_transfer_preserves_supply(address to, uint256 amount) public { uint256 t0 = token.totalSupply(); try token.transfer(to, amount) {} catch {} assert(token.totalSupply() == t0); }".to_string()
            },
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: true,
            source: Source::Tests,
            intent_text: Some(
                if literal {
                    "alice has 100 tokens after transfer".to_string()
                } else {
                    "Transferring any amount between any two addresses preserves totalSupply.".to_string()
                },
            ),
        }
    }

    fn natspec_derived_candidate(paraphrase: bool) -> SpecCandidate {
        SpecCandidate {
            name: if paraphrase {
                "check_transfer_runs".to_string()
            } else {
                "check_transfer_state_invariant".to_string()
            },
            halmos: if paraphrase {
                "function check_transfer_runs() public { token.transfer(bob, 1); assert(true); }".to_string()
            } else {
                "function check_transfer_state_invariant(address to, uint256 amount) public { uint256 t0 = token.totalSupply(); try token.transfer(to, amount) {} catch {} assert(token.totalSupply() == t0); }".to_string()
            },
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: true,
            source: Source::NatSpec,
            intent_text: Some(
                if paraphrase {
                    "Transfers tokens between accounts.".to_string()
                } else {
                    "Any successful transfer preserves the contract's totalSupply.".to_string()
                },
            ),
        }
    }

    fn write_structured_fixture(
        dir: &std::path::Path,
        req: &StructuredRequest,
        value: serde_json::Value,
    ) {
        let sha = sha_hex(&request_sha(req));
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("{sha}.json"));
        let body = serde_json::json!({
            "kind": "structured",
            "value": value,
        });
        std::fs::write(path, serde_json::to_string_pretty(&body).unwrap()).unwrap();
    }

    fn build_critique_request(
        candidate: &SpecCandidate,
        intent: &str,
        cfg: &CritiqueConfig,
    ) -> StructuredRequest {
        let prompt = render(intent, candidate, None, cfg.min_axis).expect("render");
        StructuredRequest {
            model: cfg.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: prompt,
            }],
            system: Some(
                "You are an adversarial reviewer of formal specifications. Return ONLY the JSON object the user prompt's schema describes."
                    .to_string(),
            ),
            temperature: cfg.temperature,
            max_tokens: cfg.max_tokens,
            schema_name: "Critique".to_string(),
            schema: critique_schema(),
        }
    }

    #[test]
    fn render_threads_source_provenance_for_tests() {
        let c = tests_derived_candidate(false);
        let out = render("ERC-20 conformance", &c, None, 0.5).unwrap();
        assert!(out.contains("tests"), "source_kind missing in prompt");
        assert!(
            out.contains("Phase 4 §3.4a"),
            "source_guidance missing — got {} chars",
            out.len()
        );
        assert!(
            out.contains("Transferring any amount"),
            "derived_intent missing in prompt"
        );
    }

    #[test]
    fn render_threads_source_provenance_for_natspec() {
        let c = natspec_derived_candidate(false);
        let out = render("ERC-20", &c, None, 0.5).unwrap();
        assert!(out.contains("natspec"));
        assert!(out.contains("Phase 4 §3.4b"));
    }

    #[test]
    fn render_user_intent_source_says_axis_does_not_apply() {
        // V1 path: candidate.source = UserIntent. The prompt should
        // instruct the LLM to score restate_the_source = 1.0
        // unconditionally so V1 behavior is unchanged.
        let c = sample_candidate();
        let out = render("intent", &c, None, 0.5).unwrap();
        assert!(out.contains("user_intent"));
        assert!(out.contains("does not apply"));
        assert!(out.contains("1.0"));
    }

    #[tokio::test]
    async fn tests_derived_literal_restatement_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = CritiqueConfig::for_tests();
        let candidate = tests_derived_candidate(true); // literal=true
        let req = build_critique_request(&candidate, "ERC-20 transfer", &cfg);
        // Critic returns low restate_the_source — the only axis failing.
        write_structured_fixture(
            tmp.path(),
            &req,
            serde_json::json!({
                "verdict": "reject",
                "scores": {
                    "vacuity": 0.7,
                    "body_independence": 0.6,
                    "testability": 0.6,
                    "restate_the_source": 0.2
                },
                "rationale": "Candidate hardcodes alice's address and 100 tokens — restate-the-source: this is the literal assertEq from the test, not a generalized invariant."
            }),
        );
        let critic = Critic::new(
            Arc::new(MockProvider::new(tmp.path())),
            ProviderId::Anthropic,
            cfg,
        );
        let results = critic.critique_all(&[candidate.clone()], "ERC-20 transfer", None).await;
        assert_eq!(results.len(), 1);
        let result = &results[0];
        assert!(result.scores.restate_the_source < 0.5);
        let outcome = critic.filter_accepted(vec![candidate], results);
        assert_eq!(outcome.kept.len(), 0);
        assert_eq!(outcome.dropped.len(), 1);
        assert!(outcome.dropped[0].1.rationale.contains("restate-the-source"));
    }

    #[tokio::test]
    async fn natspec_derived_paraphrase_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = CritiqueConfig::for_tests();
        let candidate = natspec_derived_candidate(true); // paraphrase=true
        let req = build_critique_request(&candidate, "ERC-20 docs", &cfg);
        write_structured_fixture(
            tmp.path(),
            &req,
            serde_json::json!({
                "verdict": "reject",
                "scores": {
                    "vacuity": 0.2,
                    "body_independence": 0.3,
                    "testability": 0.2,
                    "restate_the_source": 0.1
                },
                "rationale": "The spec asserts true after calling transfer — this is restate-the-source for the @notice doc comment with no state-level invariant."
            }),
        );
        let critic = Critic::new(
            Arc::new(MockProvider::new(tmp.path())),
            ProviderId::Anthropic,
            cfg,
        );
        let results = critic.critique_all(&[candidate.clone()], "ERC-20 docs", None).await;
        let outcome = critic.filter_accepted(vec![candidate], results);
        assert_eq!(outcome.kept.len(), 0);
        assert_eq!(outcome.dropped.len(), 1);
    }

    #[tokio::test]
    async fn tests_derived_generalization_is_accepted() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = CritiqueConfig::for_tests();
        let candidate = tests_derived_candidate(false); // generalized
        let req = build_critique_request(&candidate, "ERC-20 transfer", &cfg);
        write_structured_fixture(
            tmp.path(),
            &req,
            serde_json::json!({
                "verdict": "accept",
                "scores": {
                    "vacuity": 0.9,
                    "body_independence": 0.9,
                    "testability": 0.9,
                    "restate_the_source": 0.9
                },
                "rationale": "The spec quantifies over symbolic `to` and `amount` and asserts a state-level invariant (totalSupply preservation). Real generalization from the test's assertEq."
            }),
        );
        let critic = Critic::new(
            Arc::new(MockProvider::new(tmp.path())),
            ProviderId::Anthropic,
            cfg,
        );
        let results = critic.critique_all(&[candidate.clone()], "ERC-20 transfer", None).await;
        let outcome = critic.filter_accepted(vec![candidate], results);
        assert_eq!(outcome.kept.len(), 1);
        assert_eq!(outcome.dropped.len(), 0);
        // Source provenance survives the round-trip.
        assert_eq!(outcome.kept[0].0.source, Source::Tests);
        assert!(outcome.kept[0].0.intent_text.is_some());
    }

    #[test]
    fn restate_the_source_serde_default_keeps_v1_cassettes_compatible() {
        // A V1 cassette that returns only 3 axes must still deserialize —
        // restate_the_source defaults to 1.0 so V1 candidates aren't
        // rejected on an axis the V1 critic doesn't know about.
        let json = r#"{
            "verdict": "accept",
            "scores": {"vacuity": 0.9, "body_independence": 0.8, "testability": 0.9},
            "rationale": "ok"
        }"#;
        let parsed: CritiqueResult = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.scores.restate_the_source, 1.0);
    }
}
