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
    }

    async fn critique_one(
        &self,
        candidate: &SpecCandidate,
        intent: &str,
        description: Option<&str>,
    ) -> CritiqueResult {
        let prompt = match render(intent, candidate, description, self.cfg.min_axis) {
            Ok(p) => p,
            Err(e) => return reject_with(format!("prompt render failed: {e}")),
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
                        reject_with(format!("critique JSON shape: {e}"))
                    }
                }
            }
            Err(e) => {
                tracing::warn!("critique LLM error for {}: {e}", candidate.name);
                reject_with(format!("{e}"))
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
    let mut vars: BTreeMap<&str, &str> = BTreeMap::new();
    vars.insert("intent", intent);
    vars.insert("description", desc);
    vars.insert("spec_source", &spec_source);
    vars.insert("min_axis", threshold.as_str());
    CRITIQUE.render(&vars)
}

fn critique_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["verdict", "scores", "rationale"],
        "properties": {
            "verdict": { "type": "string", "enum": ["accept", "reject"] },
            "scores": {
                "type": "object",
                "required": ["vacuity", "body_independence", "testability"],
                "properties": {
                    "vacuity": { "type": "number", "minimum": 0, "maximum": 1 },
                    "body_independence": { "type": "number", "minimum": 0, "maximum": 1 },
                    "testability": { "type": "number", "minimum": 0, "maximum": 1 }
                }
            },
            "rationale": { "type": "string" }
        }
    })
}

fn reject_with(reason: String) -> CritiqueResult {
    CritiqueResult {
        verdict: Verdict::Reject,
        scores: CritiqueScores {
            vacuity: 0.0,
            body_independence: 0.0,
            testability: 0.0,
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
                },
                rationale: "ok".into(),
            },
            CritiqueResult {
                verdict: Verdict::Reject,
                scores: CritiqueScores {
                    vacuity: 0.1,
                    body_independence: 0.1,
                    testability: 0.1,
                },
                rationale: "vacuous spec".into(),
            },
            CritiqueResult {
                verdict: Verdict::Reject,
                scores: CritiqueScores {
                    vacuity: 0.0,
                    body_independence: 0.0,
                    testability: 0.0,
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
        };
        let low_vac = CritiqueScores {
            vacuity: 0.4,
            body_independence: 0.9,
            testability: 0.9,
        };
        assert!(critic.passes_axes(&ok));
        assert!(!critic.passes_axes(&low_vac));
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
}
