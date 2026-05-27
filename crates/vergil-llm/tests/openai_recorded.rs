//! Recorded coverage of [`vergil_llm::openai::OpenAiClient`] driven by a
//! wiremock server. Covers happy path, retry on 429/5xx, no-retry on 401,
//! tool (function) call structured-output parsing, and embeddings.

mod common;

use std::time::Duration;

use serde_json::json;
use vergil_llm::{
    openai::OpenAiClient, CompletionRequest, EmbedRequest, LlmError, LlmProvider, Message, Role,
    StructuredRequest,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn user_req() -> CompletionRequest {
    CompletionRequest {
        model: "gpt-4o-mini".to_string(),
        messages: vec![Message {
            role: Role::User,
            content: "ping".to_string(),
        }],
        system: None,
        temperature: 0.0,
        max_tokens: 100,
    }
}

fn happy_chat_response() -> serde_json::Value {
    json!({
        "id": "chatcmpl_abc",
        "object": "chat.completion",
        "created": 1_700_000_000,
        "model": "gpt-4o-mini",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "pong" },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 5, "completion_tokens": 1, "total_tokens": 6 }
    })
}

fn build_client(server: &MockServer) -> OpenAiClient {
    OpenAiClient::with_base_url("test-key", server.uri())
}

#[tokio::test]
async fn complete_happy_path_returns_text_and_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(happy_chat_response()))
        .mount(&server)
        .await;

    let client = build_client(&server);
    let resp = client.complete(user_req()).await.expect("ok");
    assert_eq!(resp.content, "pong");
    assert_eq!(resp.tokens_in, 5);
    assert_eq!(resp.tokens_out, 1);
    assert_eq!(resp.provider_request_id.as_deref(), Some("chatcmpl_abc"));
}

#[tokio::test]
async fn complete_retries_429_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(429).set_body_json(json!({ "error": { "message": "rate" } })),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(happy_chat_response()))
        .mount(&server)
        .await;

    let client = build_client(&server);
    let resp = tokio::time::timeout(Duration::from_secs(5), client.complete(user_req()))
        .await
        .expect("did not hang")
        .expect("ok after retry");
    assert_eq!(resp.content, "pong");
}

#[tokio::test]
async fn complete_retries_500_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(500).set_body_json(json!({ "error": { "message": "upstream" } })),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(happy_chat_response()))
        .mount(&server)
        .await;

    let client = build_client(&server);
    let resp = client.complete(user_req()).await.expect("ok after retry");
    assert_eq!(resp.tokens_in, 5);
}

#[tokio::test]
async fn complete_does_not_retry_401() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({ "error": { "message": "invalid key" } })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = build_client(&server);
    let err = client.complete(user_req()).await.unwrap_err();
    assert!(matches!(err, LlmError::Auth(_)), "{err}");
}

#[tokio::test]
async fn complete_structured_parses_function_arguments_json() {
    let server = MockServer::start().await;
    let tool_response = json!({
        "id": "chatcmpl_tool_1",
        "object": "chat.completion",
        "created": 1_700_000_000,
        "model": "gpt-4o-mini",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "Diagnosis",
                        "arguments": "{\"class\":\"CodeBug\",\"rationale\":\"missing allowance check\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": { "prompt_tokens": 50, "completion_tokens": 20, "total_tokens": 70 }
    });
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tool_response))
        .mount(&server)
        .await;

    let client = build_client(&server);
    let req = StructuredRequest {
        model: "gpt-4o-mini".to_string(),
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
}

#[tokio::test]
async fn complete_structured_missing_tool_call_is_schema_error() {
    let server = MockServer::start().await;
    // Response missing tool_calls entirely.
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(happy_chat_response()))
        .mount(&server)
        .await;

    let client = build_client(&server);
    let req = StructuredRequest {
        model: "gpt-4o-mini".to_string(),
        messages: vec![Message {
            role: Role::User,
            content: "x".to_string(),
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
async fn embed_returns_vectors_and_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "data": [
                { "object": "embedding", "embedding": [0.1, 0.2, 0.3], "index": 0 },
                { "object": "embedding", "embedding": [0.4, 0.5, 0.6], "index": 1 }
            ],
            "model": "text-embedding-3-small",
            "usage": { "prompt_tokens": 8, "total_tokens": 8 }
        })))
        .mount(&server)
        .await;

    let client = build_client(&server);
    let resp = client
        .embed(EmbedRequest {
            model: "text-embedding-3-small".to_string(),
            input: vec!["a".to_string(), "b".to_string()],
        })
        .await
        .expect("embed ok");
    assert_eq!(resp.vectors.len(), 2);
    assert_eq!(resp.vectors[0], vec![0.1_f32, 0.2, 0.3]);
    assert_eq!(resp.tokens, 8);
}

#[tokio::test]
async fn openai_live_smoke() {
    if !cfg!(feature = "llm-live") {
        return;
    }
    common::init_live_env();
    let Ok(key) =
        std::env::var("VERGIL_OPENAI_API_KEY").or_else(|_| std::env::var("OPENAI_API_KEY"))
    else {
        eprintln!("openai_live_smoke: no API key in env, skipping");
        return;
    };
    let client = OpenAiClient::new(key);
    let req = CompletionRequest {
        model: "gpt-4o-mini".to_string(),
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
}
