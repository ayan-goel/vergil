//! Spec synthesis. Given an intent string, a static-analysis summary, and
//! retrieved templates, sample k candidate Halmos check_ functions +
//! SMTChecker assertions from an LLM. k=16 by default (1 at T=0.0,
//! 15 at T=0.7) per SPEC §3.1.
//!
//! The LLM is non-trusted: candidates here are *proposals*. Critique
//! (Slice 10), mutation testing (Slice 11), and manifest validation
//! (Slice 7) all filter before any candidate reaches the solver.

use std::collections::BTreeMap;
use std::sync::Arc;

use futures::future::join_all;
use serde::{Deserialize, Serialize};

use vergil_llm::prompts::SYNTHESIZE;
use vergil_llm::{CompletionRequest, LlmError, LlmProvider, Message, Role};

#[derive(Debug, Clone, Default)]
pub struct SynthesisConfig {
    pub model: String,
    pub max_tokens: u32,
    /// k. SPEC §3.1 default 16 = 1 at T=0.0 + 15 at T=0.7.
    pub samples: usize,
    pub deterministic_temp: f32,
    pub exploratory_temp: f32,
}

impl SynthesisConfig {
    pub fn default_for_anthropic() -> Self {
        // claude-sonnet-4-6 accepts temperature, preserving the SPEC §3.1
        // "1 deterministic + 15 exploratory" sampling design.
        // claude-opus-4-7 is a thinking model that rejects temperature, so
        // it would collapse the k=16 diversity ladder without a redesign.
        Self {
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 4096,
            samples: 16,
            deterministic_temp: 0.0,
            exploratory_temp: 0.7,
        }
    }

    /// Slim config for unit tests: a single deterministic sample. Keeps
    /// mock-fixture authoring tractable.
    pub fn for_tests() -> Self {
        Self {
            model: "mock".to_string(),
            max_tokens: 1024,
            samples: 1,
            deterministic_temp: 0.0,
            exploratory_temp: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecCandidate {
    pub name: String,
    pub halmos: String,
    #[serde(default)]
    pub smtchecker: String,
    #[serde(default)]
    pub template_ref: Option<String>,
    #[serde(default)]
    pub intent_satisfied: bool,
}

#[derive(Debug, Default)]
pub struct SynthesisReport {
    pub candidates: Vec<SpecCandidate>,
    /// Per-sample LLM call outcomes for cost telemetry.
    pub samples: Vec<SampleStat>,
    /// Sample indices whose response could not be parsed into candidates.
    pub parse_failures: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct SampleStat {
    pub index: usize,
    pub temperature: f32,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub latency_ms: u64,
    pub candidate_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct StaticAnalysisSummary {
    /// Free-form human-readable text the prompt embeds verbatim. Future
    /// slices will derive this from `vergil_solidity::static_analysis::analyze`.
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct RetrievedHint {
    pub template_id: String,
    pub description: String,
    pub halmos_snippet: String,
}

/// Render the synthesize.txt prompt with all placeholders filled.
/// `available_methods` is the structured block describing the contract's
/// external/public function signatures (Phase 4 Slice A3). Pass an empty
/// string and the renderer substitutes a placeholder; callers should
/// prefer `vergil_solidity::signatures::render_available_methods(&sigs)`.
pub fn render_prompt(
    intent: &str,
    available_methods: &str,
    sa: &StaticAnalysisSummary,
    retrieved: &[RetrievedHint],
    contract_source: &str,
) -> Result<String, vergil_llm::prompts::PromptError> {
    let retrieved_text = retrieved
        .iter()
        .map(|r| {
            format!(
                "---\nid: {}\ndescription: {}\nencoding:\n{}",
                r.template_id, r.description, r.halmos_snippet
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let methods_default;
    let methods = if available_methods.is_empty() {
        methods_default = "(no external functions detected — read the contract source above)";
        methods_default
    } else {
        available_methods
    };
    let mut vars: BTreeMap<&str, &str> = BTreeMap::new();
    vars.insert("intent", intent);
    vars.insert("available_methods", methods);
    vars.insert("static_analysis_summary", &sa.text);
    vars.insert("retrieved_templates", &retrieved_text);
    vars.insert("contract_source", contract_source);
    SYNTHESIZE.render(&vars)
}

/// Sample `cfg.samples` candidates from the LLM. The first sample uses
/// `deterministic_temp`; the remainder use `exploratory_temp`. Each call
/// runs in parallel via `futures::future::join_all`.
pub async fn synthesize(
    provider: Arc<dyn LlmProvider>,
    intent: &str,
    available_methods: &str,
    sa: &StaticAnalysisSummary,
    retrieved: &[RetrievedHint],
    contract_source: &str,
    cfg: &SynthesisConfig,
) -> Result<SynthesisReport, SynthesisError> {
    let prompt = render_prompt(intent, available_methods, sa, retrieved, contract_source)
        .map_err(SynthesisError::Prompt)?;

    let samples = cfg.samples.max(1);
    let mut tasks = Vec::with_capacity(samples);
    for i in 0..samples {
        let temp = if i == 0 {
            cfg.deterministic_temp
        } else {
            cfg.exploratory_temp
        };
        let req = CompletionRequest {
            model: cfg.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: prompt.clone(),
            }],
            system: Some(
                "You are a formal verification expert generating Halmos check_ functions and SMTChecker assertions for Solidity smart contracts. Reply with ONLY a JSON array of SpecCandidate objects per the user prompt's schema. No prose. No code fences."
                    .to_string(),
            ),
            temperature: temp,
            max_tokens: cfg.max_tokens,
        };
        let p = provider.clone();
        tasks.push(async move {
            let started = std::time::Instant::now();
            let res = p.complete(req).await;
            (i, temp, started.elapsed().as_millis() as u64, res)
        });
    }
    let outcomes = join_all(tasks).await;

    let mut report = SynthesisReport::default();
    for (i, temp, _wall_ms, res) in outcomes {
        match res {
            Ok(completion) => {
                let parsed = parse_candidates(&completion.content);
                let count = parsed.len();
                if count == 0 {
                    let preview: String = completion.content.chars().take(200).collect();
                    tracing::warn!(
                        "synthesize sample {i} (T={temp}): 0 candidates parsed; content_len={} preview={:?}",
                        completion.content.len(),
                        preview
                    );
                    report.parse_failures.push(i);
                }
                report.samples.push(SampleStat {
                    index: i,
                    temperature: temp,
                    tokens_in: completion.tokens_in,
                    tokens_out: completion.tokens_out,
                    latency_ms: completion.latency_ms,
                    candidate_count: count,
                });
                report.candidates.extend(parsed);
            }
            Err(e) => {
                tracing::warn!("synthesize sample {i} (T={temp}) failed: {e}");
                report.parse_failures.push(i);
                report.samples.push(SampleStat {
                    index: i,
                    temperature: temp,
                    tokens_in: 0,
                    tokens_out: 0,
                    latency_ms: 0,
                    candidate_count: 0,
                });
            }
        }
    }
    Ok(report)
}

/// Parse the LLM's response as a JSON array of `SpecCandidate`. Tolerates
/// leading/trailing whitespace and stray code fences (in case the model
/// disobeys the "no code fences" instruction).
pub fn parse_candidates(raw: &str) -> Vec<SpecCandidate> {
    let trimmed = raw.trim();
    let body = strip_code_fence(trimmed);
    let body = body.trim();
    // Try direct JSON array parse first.
    if let Ok(v) = serde_json::from_str::<Vec<SpecCandidate>>(body) {
        return v;
    }
    // Tolerate single-object response.
    if let Ok(one) = serde_json::from_str::<SpecCandidate>(body) {
        return vec![one];
    }
    // Try to extract the first balanced JSON array from the body.
    if let Some(arr) = extract_first_json_array(body) {
        if let Ok(v) = serde_json::from_str::<Vec<SpecCandidate>>(&arr) {
            return v;
        }
    }
    Vec::new()
}

fn strip_code_fence(s: &str) -> &str {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("```json").or_else(|| s.strip_prefix("```")) {
        let rest = rest.trim_start_matches('\n');
        if let Some(end) = rest.rfind("```") {
            return &rest[..end];
        }
        return rest;
    }
    s
}

fn extract_first_json_array(s: &str) -> Option<String> {
    let start = s.find('[')?;
    let mut depth = 0i32;
    let bytes = s.as_bytes();
    for (i, b) in bytes.iter().enumerate().skip(start) {
        match *b {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

#[derive(Debug, thiserror::Error)]
pub enum SynthesisError {
    #[error("prompt render: {0}")]
    Prompt(vergil_llm::prompts::PromptError),
    #[error("llm: {0}")]
    Llm(#[from] LlmError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_json_array() {
        let body =
            r#"[{"name":"check_x","halmos":"fn body","smtchecker":"","intent_satisfied":true}]"#;
        let v = parse_candidates(body);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "check_x");
    }

    #[test]
    fn tolerates_code_fence_with_json_tag() {
        let body = "```json\n[{\"name\":\"check_x\",\"halmos\":\"fn body\"}]\n```";
        let v = parse_candidates(body);
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn tolerates_plain_code_fence() {
        let body = "```\n[{\"name\":\"check_x\",\"halmos\":\"fn body\"}]\n```";
        let v = parse_candidates(body);
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn extracts_array_buried_in_prose() {
        let body = "Here you go:\n[{\"name\":\"check_x\",\"halmos\":\"fn body\"}]\nLet me know.";
        let v = parse_candidates(body);
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn parses_single_object_as_one_element_vec() {
        let body = r#"{"name":"check_x","halmos":"fn body"}"#;
        let v = parse_candidates(body);
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn empty_on_unparseable() {
        let v = parse_candidates("absolutely not json");
        assert!(v.is_empty());
    }

    #[test]
    fn render_prompt_inlines_inputs() {
        let sa = StaticAnalysisSummary {
            text: "two slots, one modifier".to_string(),
        };
        let retrieved = vec![RetrievedHint {
            template_id: "erc20-x".to_string(),
            description: "balance preservation".to_string(),
            halmos_snippet: "function check_x() public {}".to_string(),
        }];
        let out = render_prompt(
            "verify ERC20 conformance",
            "- function transfer(address to, uint256 amount) external returns (bool)",
            &sa,
            &retrieved,
            "contract source",
        )
        .unwrap();
        assert!(out.contains("verify ERC20 conformance"));
        assert!(out.contains("two slots, one modifier"));
        assert!(out.contains("erc20-x"));
        assert!(out.contains("contract source"));
        assert!(out.contains("function transfer"));
        // No leftover placeholders.
        assert!(!out.contains("{{"));
    }

    #[test]
    fn render_prompt_falls_back_to_placeholder_when_methods_empty() {
        let sa = StaticAnalysisSummary::default();
        let out = render_prompt("i", "", &sa, &[], "src").unwrap();
        assert!(out.contains("no external functions detected"));
        assert!(!out.contains("{{"));
    }
}
