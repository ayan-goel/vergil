//! Integration test for the synthesis pipeline driven by MockProvider.
//! Authors a fixture file at the SHA the prompt+request produce, then
//! verifies `synthesize()` returns the expected SpecCandidate(s).

use std::path::PathBuf;
use std::sync::Arc;

use vergil_core::synthesis::{
    parse_candidates, render_prompt, synthesize, RetrievedHint, StaticAnalysisSummary,
    SynthesisConfig,
};
use vergil_llm::mock::MockProvider;
use vergil_llm::{request_sha, sha_hex, CompletionRequest, Message, Role};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("synthesis")
}

#[tokio::test]
async fn parse_candidates_handles_real_shape() {
    let body = r#"[
        {
            "name": "check_transfer_preserves_supply",
            "halmos": "function check_transfer_preserves_supply(address to, uint256 amount) public { uint256 t0 = token.totalSupply(); try token.transfer(to, amount) {} catch {} assert(token.totalSupply() == t0); }",
            "smtchecker": "",
            "template_ref": "erc20-totalsupply-invariant",
            "intent_satisfied": true
        }
    ]"#;
    let v = parse_candidates(body);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].name, "check_transfer_preserves_supply");
    assert_eq!(
        v[0].template_ref.as_deref(),
        Some("erc20-totalsupply-invariant")
    );
    assert!(v[0].intent_satisfied);
}

#[tokio::test]
async fn synthesize_returns_candidates_from_mock_fixture() {
    let cfg = SynthesisConfig::for_tests();
    let intent = "Standard ERC-20: balances sum to totalSupply; transfers respect allowances";
    let sa = StaticAnalysisSummary {
        text: "_balances (mapping address => uint256), _totalSupply (uint256), no modifiers"
            .to_string(),
    };
    let retrieved = vec![RetrievedHint {
        template_id: "erc20-sum-of-balances".to_string(),
        description: "sum of all balances equals totalSupply".to_string(),
        halmos_snippet: "function check_transferFrom_preserves_pair_sum(...) public { ... }"
            .to_string(),
    }];
    let contract_source =
        "contract Token { mapping(address => uint256) _balances; uint256 _totalSupply; }";

    // Compute the expected request SHA so we can author the fixture.
    let prompt = render_prompt(intent, &sa, &retrieved, contract_source).expect("prompt renders");
    let req = CompletionRequest {
        model: cfg.model.clone(),
        messages: vec![Message {
            role: Role::User,
            content: prompt,
        }],
        system: Some(
            "You are a formal verification expert generating Halmos check_ functions and SMTChecker assertions for Solidity smart contracts. Reply with ONLY a JSON array of SpecCandidate objects per the user prompt's schema. No prose. No code fences."
                .to_string(),
        ),
        temperature: cfg.deterministic_temp,
        max_tokens: cfg.max_tokens,
    };
    let sha = sha_hex(&request_sha(&req));
    let expected_path = fixtures_dir().join(format!("{sha}.json"));
    assert!(
        expected_path.exists(),
        "synthesis fixture missing at {}; author the expected MockProvider response here",
        expected_path.display()
    );

    let provider = Arc::new(MockProvider::new(fixtures_dir()));
    let report = synthesize(provider, intent, &sa, &retrieved, contract_source, &cfg)
        .await
        .expect("synthesize ok");
    assert_eq!(report.samples.len(), 1);
    assert!(report.parse_failures.is_empty(), "parse failed: {report:?}");
    assert!(!report.candidates.is_empty(), "no candidates produced");
    assert!(report.candidates[0].name.starts_with("check_"));
}
