//! Fixture-driven [`LlmProvider`] used by every unit test.
//!
//! Lookup key: SHA-256 of the canonical request body. The fixture file is
//! `<dir>/<sha-hex>.json` with a tagged `kind` field selecting the response
//! shape (completion / structured / embed).
//!
//! A missing fixture is a permanent error whose message includes the
//! request SHA and a hex sample of the request body, so the test author
//! can drop the missing JSON file into the fixtures dir and re-run.

use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    request_sha, sha_hex, Completion, CompletionRequest, EmbedRequest, Embedding, LlmError,
    LlmProvider, ProviderId, StructuredRequest, StructuredResponse,
};

#[derive(Clone)]
pub struct MockProvider {
    fixtures_dir: PathBuf,
}

impl MockProvider {
    pub fn new(fixtures_dir: impl Into<PathBuf>) -> Self {
        Self {
            fixtures_dir: fixtures_dir.into(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Fixture {
    Completion {
        content: String,
        #[serde(default)]
        tokens_in: u32,
        #[serde(default)]
        tokens_out: u32,
        #[serde(default)]
        latency_ms: u64,
        #[serde(default)]
        provider_request_id: Option<String>,
    },
    Structured {
        value: serde_json::Value,
        #[serde(default)]
        tokens_in: u32,
        #[serde(default)]
        tokens_out: u32,
        #[serde(default)]
        latency_ms: u64,
        #[serde(default)]
        provider_request_id: Option<String>,
    },
    Embed {
        vectors: Vec<Vec<f32>>,
        #[serde(default)]
        tokens: u32,
        #[serde(default)]
        latency_ms: u64,
    },
}

fn load_fixture<T: Serialize>(
    dir: &std::path::Path,
    req: &T,
    kind_label: &str,
) -> Result<Fixture, LlmError> {
    let sha = request_sha(req);
    let hex = sha_hex(&sha);
    let path = dir.join(format!("{hex}.json"));
    let bytes = std::fs::read(&path).map_err(|e| {
        let body = serde_json::to_string_pretty(req).unwrap_or_default();
        LlmError::Permanent(format!(
            "mock fixture not found for {kind_label} call: {} (sha={hex}, err={e}). \
             Drop the expected response at this path. Request body:\n{body}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|e| {
        LlmError::Permanent(format!(
            "mock fixture {} is not valid JSON: {e}",
            path.display()
        ))
    })
}

#[async_trait]
impl LlmProvider for MockProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Mock
    }

    async fn complete(&self, req: CompletionRequest) -> Result<Completion, LlmError> {
        let fx = load_fixture(&self.fixtures_dir, &req, "complete")?;
        match fx {
            Fixture::Completion {
                content,
                tokens_in,
                tokens_out,
                latency_ms,
                provider_request_id,
            } => Ok(Completion {
                content,
                tokens_in,
                tokens_out,
                latency_ms,
                provider_request_id,
            }),
            other => Err(LlmError::Permanent(format!(
                "mock fixture kind mismatch: expected completion, got {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    async fn complete_structured(
        &self,
        req: StructuredRequest,
    ) -> Result<StructuredResponse, LlmError> {
        let fx = load_fixture(&self.fixtures_dir, &req, "complete_structured")?;
        match fx {
            Fixture::Structured {
                value,
                tokens_in,
                tokens_out,
                latency_ms,
                provider_request_id,
            } => Ok(StructuredResponse {
                value,
                tokens_in,
                tokens_out,
                latency_ms,
                provider_request_id,
            }),
            other => Err(LlmError::Permanent(format!(
                "mock fixture kind mismatch: expected structured, got {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    async fn embed(&self, req: EmbedRequest) -> Result<Embedding, LlmError> {
        let fx = load_fixture(&self.fixtures_dir, &req, "embed")?;
        match fx {
            Fixture::Embed {
                vectors,
                tokens,
                latency_ms,
            } => Ok(Embedding {
                vectors,
                tokens,
                latency_ms,
            }),
            other => Err(LlmError::Permanent(format!(
                "mock fixture kind mismatch: expected embed, got {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, Role};

    fn write_fixture(dir: &std::path::Path, sha_hex_str: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(format!("{sha_hex_str}.json")), body).unwrap();
    }

    fn sample_completion_req() -> CompletionRequest {
        CompletionRequest {
            model: "claude-opus-4-7".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "Synthesize a balance-preservation property.".to_string(),
            }],
            system: None,
            temperature: 0.0,
            max_tokens: 1000,
        }
    }

    #[tokio::test]
    async fn missing_fixture_returns_permanent_with_sha_and_body() {
        let tmp = tempfile::tempdir().unwrap();
        let mock = MockProvider::new(tmp.path());
        let req = sample_completion_req();
        let err = mock.complete(req.clone()).await.unwrap_err();
        match err {
            LlmError::Permanent(msg) => {
                let sha = sha_hex(&request_sha(&req));
                assert!(
                    msg.contains(&sha),
                    "error should include the request SHA: {msg}"
                );
                assert!(
                    msg.contains("Synthesize a balance-preservation property"),
                    "error should include the rendered request body: {msg}"
                );
            }
            other => panic!("expected Permanent error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn completion_fixture_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let req = sample_completion_req();
        let sha = sha_hex(&request_sha(&req));
        write_fixture(
            tmp.path(),
            &sha,
            r#"{
                "kind": "completion",
                "content": "Verified: balances sum to totalSupply.",
                "tokens_in": 50,
                "tokens_out": 12,
                "latency_ms": 800,
                "provider_request_id": "mock-req-001"
            }"#,
        );
        let mock = MockProvider::new(tmp.path());
        let resp = mock.complete(req).await.unwrap();
        assert_eq!(resp.content, "Verified: balances sum to totalSupply.");
        assert_eq!(resp.tokens_in, 50);
        assert_eq!(resp.tokens_out, 12);
        assert_eq!(resp.latency_ms, 800);
        assert_eq!(resp.provider_request_id.as_deref(), Some("mock-req-001"));
    }

    #[tokio::test]
    async fn same_request_returns_same_response_across_calls() {
        let tmp = tempfile::tempdir().unwrap();
        let req = sample_completion_req();
        let sha = sha_hex(&request_sha(&req));
        write_fixture(
            tmp.path(),
            &sha,
            r#"{ "kind": "completion", "content": "deterministic" }"#,
        );
        let mock = MockProvider::new(tmp.path());
        let a = mock.complete(req.clone()).await.unwrap();
        let b = mock.complete(req).await.unwrap();
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn structured_fixture_returns_typed_value() {
        let tmp = tempfile::tempdir().unwrap();
        let req = StructuredRequest {
            model: "claude-opus-4-7".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "diagnose this".to_string(),
            }],
            system: None,
            temperature: 0.0,
            max_tokens: 500,
            schema_name: "Diagnosis".to_string(),
            schema: serde_json::json!({"type": "object"}),
        };
        let sha = sha_hex(&request_sha(&req));
        write_fixture(
            tmp.path(),
            &sha,
            r#"{ "kind": "structured", "value": { "class": "CodeBug", "rationale": "missing allowance check" } }"#,
        );
        let mock = MockProvider::new(tmp.path());
        let resp = mock.complete_structured(req).await.unwrap();
        assert_eq!(
            resp.value["class"].as_str(),
            Some("CodeBug"),
            "{:?}",
            resp.value
        );
    }

    #[tokio::test]
    async fn embed_fixture_returns_vectors() {
        let tmp = tempfile::tempdir().unwrap();
        let req = EmbedRequest {
            model: "voyage-3".to_string(),
            input: vec!["balance preservation".to_string()],
        };
        let sha = sha_hex(&request_sha(&req));
        write_fixture(
            tmp.path(),
            &sha,
            r#"{ "kind": "embed", "vectors": [[0.1, 0.2, 0.3]], "tokens": 10, "latency_ms": 25 }"#,
        );
        let mock = MockProvider::new(tmp.path());
        let resp = mock.embed(req).await.unwrap();
        assert_eq!(resp.vectors, vec![vec![0.1f32, 0.2, 0.3]]);
        assert_eq!(resp.tokens, 10);
    }

    #[tokio::test]
    async fn kind_mismatch_is_permanent_error() {
        let tmp = tempfile::tempdir().unwrap();
        let req = sample_completion_req();
        let sha = sha_hex(&request_sha(&req));
        write_fixture(
            tmp.path(),
            &sha,
            r#"{ "kind": "embed", "vectors": [[0.1]] }"#,
        );
        let mock = MockProvider::new(tmp.path());
        let err = mock.complete(req).await.unwrap_err();
        assert!(matches!(err, LlmError::Permanent(_)), "{err}");
    }
}
