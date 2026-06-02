//! Test-derived intent extraction. SPEC §3.4a / Phase 4 Slice 3.
//!
//! Given parsed Foundry / Halmos tests (from
//! [`vergil_solidity::test_parser`]), call an LLM to generalize each
//! test's assertions into 1–N candidate INVARIANTS expressed as plain
//! English. The extracted intents flow downstream as:
//!
//!   intent_text → V1 SYNTHESIZE → V1 critique → CEGIS
//!
//! and emerge tagged [`Source::Tests`] for the stratified verdict
//! (SPEC §3.6). The intent extraction is precision-priority — we ask the
//! LLM for ≤[`TestsIntentConfig::max_candidates_per_test`] candidates per
//! test (3–5 by default; **not** the k=16 fan-out V1 uses for synthesis)
//! and the downstream critique pass is the safety net against
//! restate-the-test failure modes.
//!
//! The full end-to-end pipeline (extraction + synthesis) is wired in
//! [`extract_from_tests`]; the extraction step alone is
//! [`extract_intent_candidates`] so callers that want to inspect the
//! pre-synthesis intent list can.

use std::collections::BTreeMap;
use std::sync::Arc;

use futures::future::join_all;
use serde::{Deserialize, Serialize};

use vergil_llm::prompts::TEST_TO_PROPERTY;
use vergil_llm::{CompletionRequest, LlmProvider, Message, Role};
use vergil_solidity::test_parser::{Assertion, ParsedTest};

use crate::synthesis::{
    synthesize, RetrievedHint, SampleStat, Source, SpecCandidate, StaticAnalysisSummary,
    SynthesisConfig, SynthesisError,
};

/// One intent-text candidate extracted from a single test. Pre-synthesis
/// — the Halmos `check_` function is generated downstream by the existing
/// V1 SYNTHESIZE loop fed by [`extract_from_tests`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestIntentCandidate {
    /// Snake-case identifier the synthesizer can suffix onto `check_`.
    pub name: String,
    /// Single English sentence stating the invariant — the input to the
    /// existing V1 SYNTHESIZE prompt's `intent` placeholder.
    pub intent_text: String,
    /// One-line audit-trail rationale: how the invariant generalized
    /// from the test's assertions.
    pub rationale: String,
    /// Name of the test function this candidate was derived from.
    pub source_test_name: String,
}

#[derive(Debug, Clone)]
pub struct TestsIntentConfig {
    pub model: String,
    pub max_tokens: u32,
    /// Upper bound on candidates returned per test. Precision-priority
    /// per SPEC §11.4 — keep low so the critique pass isn't flooded
    /// with near-duplicate intents.
    pub max_candidates_per_test: usize,
    /// Slightly above zero so the LLM diversifies across candidates but
    /// stays grounded in the test's assertions.
    pub temperature: f32,
}

impl TestsIntentConfig {
    pub fn default_for_anthropic() -> Self {
        Self {
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 2048,
            max_candidates_per_test: 4,
            temperature: 0.2,
        }
    }

    pub fn for_tests() -> Self {
        Self {
            model: "mock".to_string(),
            max_tokens: 1024,
            max_candidates_per_test: 3,
            temperature: 0.0,
        }
    }
}

/// Extract intent candidates from each test in `tests`. LLM calls run
/// in parallel; per-test errors and parse failures are logged and
/// dropped (an empty list, not a hard failure — extraction is
/// best-effort).
pub async fn extract_intent_candidates(
    tests: &[ParsedTest],
    cfg: &TestsIntentConfig,
    provider: Arc<dyn LlmProvider>,
) -> Vec<TestIntentCandidate> {
    let tasks: Vec<_> = tests
        .iter()
        .map(|t| {
            let p = provider.clone();
            let cfg = cfg.clone();
            let test = t.clone();
            async move { extract_one(&test, &cfg, p).await }
        })
        .collect();
    join_all(tasks).await.into_iter().flatten().collect()
}

async fn extract_one(
    test: &ParsedTest,
    cfg: &TestsIntentConfig,
    provider: Arc<dyn LlmProvider>,
) -> Vec<TestIntentCandidate> {
    let prompt = match render_prompt(test, cfg) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                "test_to_property: prompt render failed for {}: {e}",
                test.name
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
            "You extract property invariants from a Solidity test. Reply with ONLY a JSON array per the user prompt's schema. No prose. No code fences."
                .to_string(),
        ),
        temperature: cfg.temperature,
        max_tokens: cfg.max_tokens,
    };
    let completion = match provider.complete(req).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("test_to_property LLM error for {}: {e}", test.name);
            return Vec::new();
        }
    };
    let parsed = parse_response(&completion.content);
    if parsed.is_empty() && !completion.content.trim().is_empty() {
        let preview: String = completion.content.chars().take(200).collect();
        tracing::warn!(
            "test_to_property: 0 candidates parsed for {}; content_len={} preview={:?}",
            test.name,
            completion.content.len(),
            preview
        );
    }
    parsed
        .into_iter()
        .take(cfg.max_candidates_per_test)
        .map(|raw| TestIntentCandidate {
            name: sanitize_name(&raw.name),
            intent_text: raw.intent_text,
            rationale: raw.rationale,
            source_test_name: test.name.clone(),
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

fn render_prompt(
    test: &ParsedTest,
    cfg: &TestsIntentConfig,
) -> Result<String, vergil_llm::prompts::PromptError> {
    let assertions = render_assertions(&test.assertions);
    let max_str = cfg.max_candidates_per_test.to_string();
    let doc = test.doc_comment.as_deref().unwrap_or("(no doc comment)");
    let mut vars: BTreeMap<&str, &str> = BTreeMap::new();
    vars.insert("test_name", &test.name);
    vars.insert("test_doc", doc);
    vars.insert("test_body", &test.body);
    vars.insert("test_assertions", &assertions);
    vars.insert("max_candidates", &max_str);
    TEST_TO_PROPERTY.render(&vars)
}

fn render_assertions(assertions: &[Assertion]) -> String {
    if assertions.is_empty() {
        return "(no assertion sites detected)".to_string();
    }
    assertions
        .iter()
        .map(|a| match a {
            Assertion::Eq { lhs, rhs } => format!("- assertEq({lhs}, {rhs})"),
            Assertion::True { expr } => format!("- assertTrue({expr})"),
            Assertion::False { expr } => format!("- assertFalse({expr})"),
            Assertion::ExpectRevert => "- vm.expectRevert(...)".to_string(),
            Assertion::HalmosAssert { expr } => format!("- assert({expr})"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// End-to-end wrapper: extract intents from `tests`, then run each
/// through V1's existing SYNTHESIZE loop to produce `SpecCandidate`
/// records with Halmos / SMTChecker fields populated. Candidates are
/// tagged [`Source::Tests`] and carry the source intent_text for
/// traceback. Per SPEC §11.4 the synthesis step uses the existing
/// `SynthesisConfig`; callers that want tight precision typically pass
/// [`SynthesisConfig::for_tests`] (samples=1).
#[allow(clippy::too_many_arguments)]
pub async fn extract_from_tests(
    tests: &[ParsedTest],
    cfg: &TestsIntentConfig,
    extractor: Arc<dyn LlmProvider>,
    synthesizer: Arc<dyn LlmProvider>,
    synth_cfg: &SynthesisConfig,
    available_methods: &str,
    sa: &StaticAnalysisSummary,
    retrieved: &[RetrievedHint],
    contract_source: &str,
    scaffold: &str,
) -> Result<TestsIntentReport, SynthesisError> {
    let intents = extract_intent_candidates(tests, cfg, extractor).await;
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
            c.source = Source::Tests;
            if c.intent_text.is_none() {
                c.intent_text = Some(intent.intent_text.clone());
            }
            all_candidates.push(c);
        }
    }
    Ok(TestsIntentReport {
        intents,
        candidates: all_candidates,
        synthesis_samples: synth_samples,
    })
}

#[derive(Debug, Default)]
pub struct TestsIntentReport {
    pub intents: Vec<TestIntentCandidate>,
    pub candidates: Vec<SpecCandidate>,
    pub synthesis_samples: Vec<SampleStat>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use vergil_llm::mock::MockProvider;
    use vergil_llm::{request_sha, sha_hex};
    use vergil_solidity::test_parser::Assertion;

    fn sample_test(name: &str) -> ParsedTest {
        ParsedTest {
            name: name.to_string(),
            doc_comment: Some("preserves totalSupply on transfer".to_string()),
            body: "uint256 a = token.totalSupply(); token.transfer(bob, 5); assertEq(token.totalSupply(), a);".to_string(),
            assertions: vec![Assertion::Eq {
                lhs: "token.totalSupply()".to_string(),
                rhs: "a".to_string(),
            }],
            source_path: PathBuf::new(),
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

    fn build_request(test: &ParsedTest, cfg: &TestsIntentConfig) -> CompletionRequest {
        let prompt = render_prompt(test, cfg).expect("render");
        CompletionRequest {
            model: cfg.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: prompt,
            }],
            system: Some(
                "You extract property invariants from a Solidity test. Reply with ONLY a JSON array per the user prompt's schema. No prose. No code fences."
                    .to_string(),
            ),
            temperature: cfg.temperature,
            max_tokens: cfg.max_tokens,
        }
    }

    #[tokio::test]
    async fn well_formed_json_returns_candidates() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TestsIntentConfig::for_tests();
        let test = sample_test("testTransferPreservesSupply");
        let req = build_request(&test, &cfg);
        write_completion_fixture(
            tmp.path(),
            &req,
            r#"[
                {"name": "transfer_preserves_total_supply",
                 "intent_text": "Transferring any amount between any two addresses preserves totalSupply.",
                 "rationale": "The test asserts totalSupply is identical before and after a transfer; generalize over the symbolic recipient and amount."}
            ]"#,
        );
        let provider = Arc::new(MockProvider::new(tmp.path()));
        let out = extract_intent_candidates(&[test], &cfg, provider).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "transfer_preserves_total_supply");
        assert!(out[0].intent_text.contains("preserves totalSupply"));
        assert_eq!(out[0].source_test_name, "testTransferPreservesSupply");
    }

    #[tokio::test]
    async fn malformed_response_logs_and_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TestsIntentConfig::for_tests();
        let test = sample_test("testMalformed");
        let req = build_request(&test, &cfg);
        write_completion_fixture(
            tmp.path(),
            &req,
            "this is absolutely not JSON and never will be",
        );
        let provider = Arc::new(MockProvider::new(tmp.path()));
        let out = extract_intent_candidates(&[test], &cfg, provider).await;
        assert!(out.is_empty(), "malformed JSON should yield 0 candidates");
    }

    #[tokio::test]
    async fn empty_array_returns_zero_candidates() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TestsIntentConfig::for_tests();
        let test = sample_test("testEmpty");
        let req = build_request(&test, &cfg);
        write_completion_fixture(tmp.path(), &req, "[]");
        let provider = Arc::new(MockProvider::new(tmp.path()));
        let out = extract_intent_candidates(&[test], &cfg, provider).await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn multi_candidate_response_caps_at_max() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TestsIntentConfig::for_tests(); // max=3
        let test = sample_test("testMulti");
        let req = build_request(&test, &cfg);
        write_completion_fixture(
            tmp.path(),
            &req,
            r#"[
                {"name":"a","intent_text":"first invariant","rationale":"r"},
                {"name":"b","intent_text":"second invariant","rationale":"r"},
                {"name":"c","intent_text":"third invariant","rationale":"r"},
                {"name":"d","intent_text":"fourth invariant","rationale":"r"}
            ]"#,
        );
        let provider = Arc::new(MockProvider::new(tmp.path()));
        let out = extract_intent_candidates(&[test], &cfg, provider).await;
        // max_candidates_per_test=3, so the 4th is dropped.
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].name, "a");
        assert_eq!(out[1].name, "b");
        assert_eq!(out[2].name, "c");
    }

    #[tokio::test]
    async fn tolerates_code_fence_wrapping() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TestsIntentConfig::for_tests();
        let test = sample_test("testFenced");
        let req = build_request(&test, &cfg);
        // Model disobeys the "no code fences" rule.
        write_completion_fixture(
            tmp.path(),
            &req,
            "```json\n[{\"name\":\"x\",\"intent_text\":\"y\",\"rationale\":\"r\"}]\n```",
        );
        let provider = Arc::new(MockProvider::new(tmp.path()));
        let out = extract_intent_candidates(&[test], &cfg, provider).await;
        assert_eq!(out.len(), 1);
    }

    #[tokio::test]
    async fn extracts_from_multiple_tests_in_parallel() {
        // Each test gets its own fixture; provider serves both.
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TestsIntentConfig::for_tests();
        let t1 = sample_test("testA");
        let t2 = sample_test("testB");
        for (test, intent) in &[(t1.clone(), "invariant A"), (t2.clone(), "invariant B")] {
            let req = build_request(test, &cfg);
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
        let out = extract_intent_candidates(&[t1, t2], &cfg, provider).await;
        assert_eq!(out.len(), 2);
        let sources: Vec<&str> = out.iter().map(|c| c.source_test_name.as_str()).collect();
        assert!(sources.contains(&"testA"), "{sources:?}");
        assert!(sources.contains(&"testB"), "{sources:?}");
    }

    #[test]
    fn sanitize_name_lowercases_and_replaces_non_alphanum() {
        assert_eq!(sanitize_name("TransferPreserves Supply"), "transferpreserves_supply");
        assert_eq!(sanitize_name(""), "intent");
        assert_eq!(sanitize_name("___"), "intent");
        assert_eq!(sanitize_name("ok_name_42"), "ok_name_42");
    }

    #[test]
    fn render_assertions_lists_each_variant() {
        let out = render_assertions(&[
            Assertion::Eq {
                lhs: "x".into(),
                rhs: "y".into(),
            },
            Assertion::True { expr: "a > 0".into() },
            Assertion::ExpectRevert,
            Assertion::HalmosAssert {
                expr: "a == b".into(),
            },
        ]);
        assert!(out.contains("assertEq(x, y)"));
        assert!(out.contains("assertTrue(a > 0)"));
        assert!(out.contains("vm.expectRevert"));
        assert!(out.contains("assert(a == b)"));
    }

    #[test]
    fn render_assertions_handles_empty_list() {
        let out = render_assertions(&[]);
        assert!(out.contains("no assertion sites"));
    }

    #[test]
    fn parse_response_extracts_array_from_prose() {
        let body =
            r#"Here you go:
            [{"name":"x","intent_text":"y","rationale":"z"}]
            That's it."#;
        let v = parse_response(body);
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn parse_response_tolerates_single_object() {
        let body = r#"{"name":"x","intent_text":"y","rationale":"z"}"#;
        let v = parse_response(body);
        assert_eq!(v.len(), 1);
    }
}
