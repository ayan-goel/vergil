//! NatSpec-derived intent extraction. SPEC §3.4b / Phase 4 Slice 4.
//!
//! Mirrors [`crate::tests_intent`] but reads from
//! [`vergil_solidity::natspec`] doc-comment blocks instead of test
//! assertions. Tag treatment per SPEC §3.4b:
//!
//! - `@invariant` and `@custom:security` are the strongest signals;
//!   the LLM is instructed to translate them directly with minimal
//!   reinterpretation.
//! - `@notice` and `@dev` are paraphrased intent; the LLM generalizes
//!   more aggressively.
//!
//! Extraction output: [`NatSpecIntentCandidate`] records carrying the
//! target kind (contract / function / storage) and target name so the
//! downstream report can show users which NatSpec block produced each
//! candidate. The full end-to-end pipeline (extraction + V1 synthesis)
//! is [`extract_from_natspec`]; the extraction step alone is
//! [`extract_intent_candidates_from_natspec`].

use std::collections::BTreeMap;
use std::sync::Arc;

use futures::future::join_all;
use serde::{Deserialize, Serialize};

use vergil_llm::prompts::NATSPEC_TO_PROPERTY;
use vergil_llm::{CompletionRequest, LlmProvider, Message, Role};
use vergil_solidity::natspec::{NatSpecBlock, NatSpecTarget};

use crate::synthesis::{
    synthesize, RetrievedHint, SampleStat, Source, SpecCandidate, StaticAnalysisSummary,
    SynthesisConfig, SynthesisError,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NatSpecIntentCandidate {
    pub name: String,
    pub intent_text: String,
    pub rationale: String,
    pub source_target_kind: String,
    pub source_target_name: String,
}

#[derive(Debug, Clone)]
pub struct NatSpecIntentConfig {
    pub model: String,
    pub max_tokens: u32,
    /// Per-block cap. The SPEC §11.4 floor is "≥3 NatSpec candidates
    /// per contract", which usually comes from multiple blocks, so a
    /// per-block max of 2–3 is plenty.
    pub max_candidates_per_block: usize,
    pub temperature: f32,
}

impl NatSpecIntentConfig {
    pub fn default_for_anthropic() -> Self {
        Self {
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 2048,
            max_candidates_per_block: 3,
            temperature: 0.2,
        }
    }

    pub fn for_tests() -> Self {
        Self {
            model: "mock".to_string(),
            max_tokens: 1024,
            max_candidates_per_block: 3,
            temperature: 0.0,
        }
    }
}

/// Extract intent candidates from each NatSpec block. LLM calls run in
/// parallel; per-block errors and parse failures are logged and dropped
/// (the extraction is best-effort, not the trusted base).
///
/// Blocks with no `@notice` / `@dev` / `@invariant` / `@custom:security`
/// content are pre-filtered — they have nothing for the LLM to extract.
pub async fn extract_intent_candidates_from_natspec(
    blocks: &[NatSpecBlock],
    cfg: &NatSpecIntentConfig,
    provider: Arc<dyn LlmProvider>,
) -> Vec<NatSpecIntentCandidate> {
    let actionable: Vec<&NatSpecBlock> = blocks.iter().filter(|b| is_actionable(b)).collect();
    let tasks: Vec<_> = actionable
        .into_iter()
        .map(|b| {
            let p = provider.clone();
            let cfg = cfg.clone();
            let block = b.clone();
            async move { extract_one(&block, &cfg, p).await }
        })
        .collect();
    join_all(tasks).await.into_iter().flatten().collect()
}

fn is_actionable(b: &NatSpecBlock) -> bool {
    b.notice.is_some()
        || !b.dev.is_empty()
        || !b.invariant.is_empty()
        || !b.custom_security.is_empty()
}

async fn extract_one(
    block: &NatSpecBlock,
    cfg: &NatSpecIntentConfig,
    provider: Arc<dyn LlmProvider>,
) -> Vec<NatSpecIntentCandidate> {
    let (kind, name) = target_kind_and_name(&block.target);
    let prompt = match render_prompt(block, cfg) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                "natspec_to_property: prompt render failed for {kind}/{name}: {e}"
            );
            return Vec::new();
        }
    };
    let req = CompletionRequest {
        model: cfg.model.clone(),
        messages: vec![Message {
            role: Role::User,
            content: prompt,
        }],
        system: Some(
            "You extract property invariants from a Solidity NatSpec block. Reply with ONLY a JSON array per the user prompt's schema. No prose. No code fences."
                .to_string(),
        ),
        temperature: cfg.temperature,
        max_tokens: cfg.max_tokens,
    };
    let completion = match provider.complete(req).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("natspec_to_property LLM error for {kind}/{name}: {e}");
            return Vec::new();
        }
    };
    let parsed = parse_response(&completion.content);
    if parsed.is_empty() && !completion.content.trim().is_empty() {
        let preview: String = completion.content.chars().take(200).collect();
        tracing::warn!(
            "natspec_to_property: 0 candidates parsed for {kind}/{name}; content_len={} preview={:?}",
            completion.content.len(),
            preview
        );
    }
    parsed
        .into_iter()
        .take(cfg.max_candidates_per_block)
        .map(|raw| NatSpecIntentCandidate {
            name: sanitize_name(&raw.name),
            intent_text: raw.intent_text,
            rationale: raw.rationale,
            source_target_kind: kind.to_string(),
            source_target_name: name.clone(),
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct RawCandidate {
    name: String,
    intent_text: String,
    #[serde(default)]
    rationale: String,
}

fn parse_response(raw: &str) -> Vec<RawCandidate> {
    let trimmed = raw.trim();
    let body = strip_code_fence(trimmed).trim();
    if let Ok(v) = serde_json::from_str::<Vec<RawCandidate>>(body) {
        return v;
    }
    if let Ok(one) = serde_json::from_str::<RawCandidate>(body) {
        return vec![one];
    }
    if let Some(arr) = extract_first_json_array(body) {
        if let Ok(v) = serde_json::from_str::<Vec<RawCandidate>>(&arr) {
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

fn sanitize_name(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() {
        "intent".to_string()
    } else {
        s
    }
}

fn target_kind_and_name(target: &NatSpecTarget) -> (&'static str, String) {
    match target {
        NatSpecTarget::Contract { name } => ("contract", name.clone()),
        NatSpecTarget::Function { name } => ("function", name.clone()),
        NatSpecTarget::Storage { name } => ("storage", name.clone()),
    }
}

fn render_prompt(
    block: &NatSpecBlock,
    cfg: &NatSpecIntentConfig,
) -> Result<String, vergil_llm::prompts::PromptError> {
    let (kind, name) = target_kind_and_name(&block.target);
    let max_str = cfg.max_candidates_per_block.to_string();
    let notice = block.notice.as_deref().unwrap_or("(none)");
    let dev = render_list(&block.dev);
    let invariant = render_list(&block.invariant);
    let custom_security = render_list(&block.custom_security);
    let mut vars: BTreeMap<&str, &str> = BTreeMap::new();
    vars.insert("target_kind", kind);
    vars.insert("target_name", &name);
    vars.insert("notice", notice);
    vars.insert("dev", &dev);
    vars.insert("invariant", &invariant);
    vars.insert("custom_security", &custom_security);
    vars.insert("max_candidates", &max_str);
    NATSPEC_TO_PROPERTY.render(&vars)
}

fn render_list(items: &[String]) -> String {
    if items.is_empty() {
        return "(none)".to_string();
    }
    items
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// End-to-end wrapper: extract from NatSpec, then run each intent
/// through V1's existing SYNTHESIZE loop. Returned candidates carry
/// [`Source::NatSpec`] and the source intent_text.
#[allow(clippy::too_many_arguments)]
pub async fn extract_from_natspec(
    blocks: &[NatSpecBlock],
    cfg: &NatSpecIntentConfig,
    extractor: Arc<dyn LlmProvider>,
    synthesizer: Arc<dyn LlmProvider>,
    synth_cfg: &SynthesisConfig,
    available_methods: &str,
    sa: &StaticAnalysisSummary,
    retrieved: &[RetrievedHint],
    contract_source: &str,
    scaffold: &str,
) -> Result<NatSpecIntentReport, SynthesisError> {
    let intents = extract_intent_candidates_from_natspec(blocks, cfg, extractor).await;
    let mut all_candidates = Vec::new();
    let mut synth_samples = Vec::new();
    for intent in &intents {
        let report = synthesize(
            synthesizer.clone(),
            &intent.intent_text,
            available_methods,
            sa,
            retrieved,
            contract_source,
            scaffold,
            synth_cfg,
        )
        .await?;
        synth_samples.extend(report.samples);
        for mut c in report.candidates {
            c.source = Source::NatSpec;
            if c.intent_text.is_none() {
                c.intent_text = Some(intent.intent_text.clone());
            }
            all_candidates.push(c);
        }
    }
    Ok(NatSpecIntentReport {
        intents,
        candidates: all_candidates,
        synthesis_samples: synth_samples,
    })
}

#[derive(Debug, Default)]
pub struct NatSpecIntentReport {
    pub intents: Vec<NatSpecIntentCandidate>,
    pub candidates: Vec<SpecCandidate>,
    pub synthesis_samples: Vec<SampleStat>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use vergil_llm::mock::MockProvider;
    use vergil_llm::{request_sha, sha_hex};
    use vergil_solidity::natspec::{NatSpecBlock, NatSpecTarget, SourceSpan};

    fn block_with_notice() -> NatSpecBlock {
        NatSpecBlock {
            target: NatSpecTarget::Function {
                name: "transfer".to_string(),
            },
            notice: Some("transfers tokens between accounts".to_string()),
            dev: vec!["assumes the caller has been approved".to_string()],
            invariant: vec![],
            custom_security: vec![],
            source_span: SourceSpan::default(),
        }
    }

    fn block_with_invariant() -> NatSpecBlock {
        NatSpecBlock {
            target: NatSpecTarget::Contract {
                name: "Vault".to_string(),
            },
            notice: None,
            dev: vec![],
            invariant: vec!["totalSupply equals sum of balanceOf[*]".to_string()],
            custom_security: vec![],
            source_span: SourceSpan::default(),
        }
    }

    fn block_with_custom_security() -> NatSpecBlock {
        NatSpecBlock {
            target: NatSpecTarget::Function {
                name: "adminWithdraw".to_string(),
            },
            notice: None,
            dev: vec![],
            invariant: vec![],
            custom_security: vec!["only the owner can call this function".to_string()],
            source_span: SourceSpan::default(),
        }
    }

    fn empty_block() -> NatSpecBlock {
        NatSpecBlock {
            target: NatSpecTarget::Contract {
                name: "EmptyDoc".to_string(),
            },
            notice: None,
            dev: vec![],
            invariant: vec![],
            custom_security: vec![],
            source_span: SourceSpan::default(),
        }
    }

    fn write_completion_fixture(dir: &std::path::Path, req: &CompletionRequest, body: &str) {
        let sha = sha_hex(&request_sha(req));
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("{sha}.json"));
        let json = format!(
            r#"{{ "kind": "completion", "content": {} }}"#,
            serde_json::to_string(body).unwrap()
        );
        std::fs::write(path, json).unwrap();
    }

    fn build_request(block: &NatSpecBlock, cfg: &NatSpecIntentConfig) -> CompletionRequest {
        let prompt = render_prompt(block, cfg).expect("render");
        CompletionRequest {
            model: cfg.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: prompt,
            }],
            system: Some(
                "You extract property invariants from a Solidity NatSpec block. Reply with ONLY a JSON array per the user prompt's schema. No prose. No code fences."
                    .to_string(),
            ),
            temperature: cfg.temperature,
            max_tokens: cfg.max_tokens,
        }
    }

    #[tokio::test]
    async fn well_formed_json_returns_candidates_with_target_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = NatSpecIntentConfig::for_tests();
        let block = block_with_notice();
        let req = build_request(&block, &cfg);
        write_completion_fixture(
            tmp.path(),
            &req,
            r#"[{
                "name": "transfer_preserves_supply",
                "intent_text": "Any successful transfer preserves totalSupply.",
                "rationale": "Generalized from the @notice/@dev pair on the transfer function."
            }]"#,
        );
        let provider = Arc::new(MockProvider::new(tmp.path()));
        let out = extract_intent_candidates_from_natspec(&[block], &cfg, provider).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source_target_kind, "function");
        assert_eq!(out[0].source_target_name, "transfer");
    }

    #[tokio::test]
    async fn invariant_tag_block_extracts_direct_property() {
        // Per SPEC §3.4b, @invariant tags receive direct-property
        // treatment: the LLM is instructed to preserve the author's
        // wording. We simulate that by returning a fixture whose
        // intent_text matches the @invariant literally.
        let tmp = tempfile::tempdir().unwrap();
        let cfg = NatSpecIntentConfig::for_tests();
        let block = block_with_invariant();
        let req = build_request(&block, &cfg);
        write_completion_fixture(
            tmp.path(),
            &req,
            r#"[{
                "name": "supply_equals_sum_of_balances",
                "intent_text": "totalSupply equals the sum of balanceOf[*] at all times.",
                "rationale": "Direct extraction from the @invariant tag on contract Vault."
            }]"#,
        );
        let provider = Arc::new(MockProvider::new(tmp.path()));
        let out = extract_intent_candidates_from_natspec(&[block], &cfg, provider).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source_target_kind, "contract");
        assert_eq!(out[0].source_target_name, "Vault");
        assert!(out[0].intent_text.contains("sum of balanceOf"));
        assert!(out[0].rationale.contains("@invariant"));
    }

    #[tokio::test]
    async fn custom_security_tag_block_is_processed() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = NatSpecIntentConfig::for_tests();
        let block = block_with_custom_security();
        let req = build_request(&block, &cfg);
        write_completion_fixture(
            tmp.path(),
            &req,
            r#"[{
                "name": "only_owner_can_admin_withdraw",
                "intent_text": "adminWithdraw reverts when called by anyone other than owner.",
                "rationale": "Direct extraction from the @custom:security tag on adminWithdraw."
            }]"#,
        );
        let provider = Arc::new(MockProvider::new(tmp.path()));
        let out = extract_intent_candidates_from_natspec(&[block], &cfg, provider).await;
        assert_eq!(out.len(), 1);
        assert!(out[0].intent_text.contains("adminWithdraw"));
    }

    #[tokio::test]
    async fn empty_block_is_filtered_out_pre_llm() {
        // No content → no LLM call → no result. The provider has no
        // fixture; if extract_one had been called, MockProvider would
        // error with "fixture not found".
        let tmp = tempfile::tempdir().unwrap();
        let cfg = NatSpecIntentConfig::for_tests();
        let provider = Arc::new(MockProvider::new(tmp.path()));
        let out = extract_intent_candidates_from_natspec(&[empty_block()], &cfg, provider).await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn malformed_response_logs_and_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = NatSpecIntentConfig::for_tests();
        let block = block_with_notice();
        let req = build_request(&block, &cfg);
        write_completion_fixture(tmp.path(), &req, "definitely not JSON");
        let provider = Arc::new(MockProvider::new(tmp.path()));
        let out = extract_intent_candidates_from_natspec(&[block], &cfg, provider).await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn multi_candidate_response_caps_at_max() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = NatSpecIntentConfig::for_tests(); // max=3
        let block = block_with_invariant();
        let req = build_request(&block, &cfg);
        write_completion_fixture(
            tmp.path(),
            &req,
            r#"[
                {"name":"a","intent_text":"first","rationale":"r"},
                {"name":"b","intent_text":"second","rationale":"r"},
                {"name":"c","intent_text":"third","rationale":"r"},
                {"name":"d","intent_text":"fourth","rationale":"r"}
            ]"#,
        );
        let provider = Arc::new(MockProvider::new(tmp.path()));
        let out = extract_intent_candidates_from_natspec(&[block], &cfg, provider).await;
        assert_eq!(out.len(), 3);
    }

    #[tokio::test]
    async fn multiple_blocks_in_parallel() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = NatSpecIntentConfig::for_tests();
        let b1 = block_with_notice();
        let b2 = block_with_invariant();
        for (block, intent) in &[(b1.clone(), "first inv"), (b2.clone(), "second inv")] {
            let req = build_request(block, &cfg);
            write_completion_fixture(
                tmp.path(),
                &req,
                &format!(
                    r#"[{{"name":"x","intent_text":{},"rationale":"r"}}]"#,
                    serde_json::to_string(intent).unwrap()
                ),
            );
        }
        let provider = Arc::new(MockProvider::new(tmp.path()));
        let out = extract_intent_candidates_from_natspec(&[b1, b2], &cfg, provider).await;
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn render_list_handles_empty_and_multi() {
        assert!(render_list(&[]).contains("none"));
        let multi = vec!["a".to_string(), "b".to_string()];
        let out = render_list(&multi);
        assert!(out.contains("- a"));
        assert!(out.contains("- b"));
    }

    #[test]
    fn target_kind_returns_lowercase_label() {
        let (k, n) = target_kind_and_name(&NatSpecTarget::Contract {
            name: "Vault".into(),
        });
        assert_eq!(k, "contract");
        assert_eq!(n, "Vault");
        let (k, _) = target_kind_and_name(&NatSpecTarget::Function {
            name: "f".into(),
        });
        assert_eq!(k, "function");
        let (k, _) = target_kind_and_name(&NatSpecTarget::Storage {
            name: "s".into(),
        });
        assert_eq!(k, "storage");
    }

    #[test]
    fn render_prompt_inlines_all_tag_categories() {
        let block = NatSpecBlock {
            target: NatSpecTarget::Contract {
                name: "C".into(),
            },
            notice: Some("notice text".into()),
            dev: vec!["dev line one".into()],
            invariant: vec!["invariant one".into()],
            custom_security: vec!["security note".into()],
            source_span: SourceSpan::default(),
        };
        let cfg = NatSpecIntentConfig::for_tests();
        let out = render_prompt(&block, &cfg).unwrap();
        assert!(out.contains("notice text"));
        assert!(out.contains("dev line one"));
        assert!(out.contains("invariant one"));
        assert!(out.contains("security note"));
        assert!(!out.contains("{{"));
    }
}
