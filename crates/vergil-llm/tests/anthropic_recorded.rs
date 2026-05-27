//! Recorded coverage of [`vergil_llm::anthropic::AnthropicClient`] driven by
//! a wiremock server. Every test stubs the Anthropic /v1/messages endpoint
//! with canned bytes, then asserts the LlmProvider surface behaves correctly.
//! No network. Validates: success path, retry on 429 + 5xx, no-retry on 401,
//! and structured (tool-use) response parsing.

mod common;

use std::time::Duration;

use serde_json::json;
use vergil_llm::{
    anthropic::AnthropicClient, CompletionRequest, EmbedRequest, LlmError, LlmProvider, Message,
    Role, StructuredRequest,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn user_req() -> CompletionRequest {
    CompletionRequest {
        model: "claude-opus-4-7".to_string(),
        messages: vec![Message {
            role: Role::User,
            content: "hello".to_string(),
        }],
        system: None,
        temperature: 0.0,
        max_tokens: 100,
    }
}

fn happy_response() -> serde_json::Value {
    json!({
        "id": "msg_abc123",
        "type": "message",
        "role": "assistant",
        "model": "claude-opus-4-7",
        "content": [
            { "type": "text", "text": "world" }
        ],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": { "input_tokens": 11, "output_tokens": 2 }
    })
}

async fn build_client(server: &MockServer) -> AnthropicClient {
    // The SDK appends "/messages" to the configured base URL. Mirror the
    // canonical production base URL shape (`<host>/v1`) so the request URL
    // we're matching against in wiremock matches what production looks like.
    AnthropicClient::with_base_url("test-api-key", format!("{}/v1", server.uri()))
        .expect("client builds")
}

#[tokio::test]
async fn complete_happy_path_returns_text_and_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(happy_response()))
        .mount(&server)
        .await;

    let client = build_client(&server).await;
    let resp = client.complete(user_req()).await.expect("ok");
    assert_eq!(resp.content, "world");
    assert_eq!(resp.tokens_in, 11);
    assert_eq!(resp.tokens_out, 2);
    assert_eq!(resp.provider_request_id.as_deref(), Some("msg_abc123"));
}

#[tokio::test]
async fn complete_retries_429_then_succeeds() {
    let server = MockServer::start().await;
    // First call: 429. Subsequent: 200.
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(429).set_body_string("{\"error\": \"rate limit\"}"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(happy_response()))
        .mount(&server)
        .await;

    let client = build_client(&server).await;
    let started = std::time::Instant::now();
    let resp = tokio::time::timeout(Duration::from_secs(5), client.complete(user_req()))
        .await
        .expect("did not hang")
        .expect("ok after retry");
    assert_eq!(resp.content, "world");
    // Backoff is ~500ms + jitter; sanity check it slept at least a bit.
    assert!(started.elapsed() >= Duration::from_millis(300));
}

#[tokio::test]
async fn complete_retries_500_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(500).set_body_string("{\"error\": \"upstream\"}"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(happy_response()))
        .mount(&server)
        .await;

    let client = build_client(&server).await;
    let resp = tokio::time::timeout(Duration::from_secs(5), client.complete(user_req()))
        .await
        .expect("did not hang")
        .expect("ok after retry");
    assert_eq!(resp.tokens_in, 11);
}

#[tokio::test]
async fn complete_does_not_retry_401() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_string("{\"error\": \"unauthorized\"}"))
        .expect(1)
        .mount(&server)
        .await;

    let client = build_client(&server).await;
    let err = client.complete(user_req()).await.unwrap_err();
    assert!(matches!(err, LlmError::Auth(_)), "{err}");
    // Mock's .expect(1) guarantees exactly-once dispatch — verified on drop.
}

#[tokio::test]
async fn complete_structured_parses_tool_use_input() {
    let server = MockServer::start().await;
    let tool_response = json!({
        "id": "msg_tool_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-opus-4-7",
        "content": [
            {
                "type": "tool_use",
                "id": "toolu_1",
                "name": "Diagnosis",
                "input": { "class": "CodeBug", "rationale": "missing allowance check" }
            }
        ],
        "stop_reason": "tool_use",
        "stop_sequence": null,
        "usage": { "input_tokens": 50, "output_tokens": 20 }
    });
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tool_response))
        .mount(&server)
        .await;

    let client = build_client(&server).await;
    let req = StructuredRequest {
        model: "claude-opus-4-7".to_string(),
        messages: vec![Message {
            role: Role::User,
            content: "diagnose".to_string(),
        }],
        system: None,
        temperature: 0.0,
        max_tokens: 200,
        schema_name: "Diagnosis".to_string(),
        schema: json!({ "type": "object" }),
    };
    let resp = client
        .complete_structured(req)
        .await
        .expect("structured ok");
    assert_eq!(resp.value["class"], "CodeBug");
    assert_eq!(resp.tokens_in, 50);
    assert_eq!(resp.provider_request_id.as_deref(), Some("msg_tool_1"));
}

#[tokio::test]
async fn complete_structured_missing_tool_use_is_schema_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(happy_response()))
        .mount(&server)
        .await;

    let client = build_client(&server).await;
    let req = StructuredRequest {
        model: "claude-opus-4-7".to_string(),
        messages: vec![Message {
            role: Role::User,
            content: "diagnose".to_string(),
        }],
        system: None,
        temperature: 0.0,
        max_tokens: 200,
        schema_name: "Diagnosis".to_string(),
        schema: json!({ "type": "object" }),
    };
    let err = client.complete_structured(req).await.unwrap_err();
    assert!(matches!(err, LlmError::Schema(_)), "{err}");
}

#[tokio::test]
async fn embed_returns_permanent_unsupported() {
    // No server stub — embed must not even attempt an HTTP call.
    let server = MockServer::start().await;
    let client = build_client(&server).await;
    let err = client
        .embed(EmbedRequest {
            model: "voyage-3".into(),
            input: vec!["x".into()],
        })
        .await
        .unwrap_err();
    assert!(matches!(err, LlmError::Permanent(_)), "{err}");
}

#[tokio::test]
async fn anthropic_live_smoke() {
    if !cfg!(feature = "llm-live") {
        return; // gate: only run when --features llm-live is set
    }
    common::init_live_env();
    let Ok(key) =
        std::env::var("VERGIL_ANTHROPIC_API_KEY").or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
    else {
        eprintln!("anthropic_live_smoke: no API key in env, skipping");
        return;
    };
    let client = AnthropicClient::new(key).expect("client builds");
    let req = CompletionRequest {
        model: "claude-haiku-4-5-20251001".to_string(),
        messages: vec![Message {
            role: Role::User,
            content: "Reply with the single word: ok".to_string(),
        }],
        system: Some("You are a terse assistant.".to_string()),
        temperature: 0.0,
        max_tokens: 32,
    };
    let resp = client.complete(req).await.expect("live ok");
    assert!(!resp.content.is_empty());
    assert!(resp.tokens_in > 0);
    assert!(resp.tokens_out > 0);
}
