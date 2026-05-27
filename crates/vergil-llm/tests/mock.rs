//! Integration coverage for [`vergil_llm::mock::MockProvider`] driving the
//! committed fixtures under `tests/fixtures/mock/`.

mod common;

use std::path::PathBuf;

use vergil_llm::{
    mock::MockProvider, request_sha, sha_hex, CompletionRequest, LlmProvider, Message, Role,
};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mock")
}

#[tokio::test]
async fn synthesize_v1_fixture_loads() {
    let req = CompletionRequest {
        model: "claude-opus-4-7".to_string(),
        messages: vec![Message {
            role: Role::User,
            content: "Synthesize a Halmos check_ function for balance preservation on an ERC-20."
                .to_string(),
        }],
        system: Some("You are a formal verification expert.".to_string()),
        temperature: 0.0,
        max_tokens: 2000,
    };
    // The committed fixture is named for this exact request SHA. If it
    // ever drifts, the assertion below will fail with the expected SHA so
    // the fixture can be renamed in one step.
    let expected_sha = sha_hex(&request_sha(&req));
    assert!(
        fixtures_dir().join(format!("{expected_sha}.json")).exists(),
        "committed fixture file must match request SHA {expected_sha}"
    );

    let mock = MockProvider::new(fixtures_dir());
    let resp = mock.complete(req).await.expect("fixture loads");
    assert!(
        resp.content.contains("check_"),
        "synthesize fixture should produce Halmos check_ function text"
    );
    assert!(resp.tokens_in > 0);
    assert!(resp.tokens_out > 0);
}

#[tokio::test]
async fn live_env_helper_compiles_and_runs() {
    common::init_live_env();
    // We don't assert any var is set — the helper's job is just to attempt
    // loading without panicking. Whether keys are present depends on the
    // contributor's local setup.
}
