//! Provider-agnostic LLM client surface for Vergil.
//!
//! The [`LlmProvider`] trait is the single integration seam every downstream
//! crate (synthesis, critique, diagnosis) talks to. Real providers live in
//! `anthropic.rs` (Slice 2) and `openai.rs` (Slice 3); [`mock::MockProvider`]
//! drives every unit test deterministically from fixture files. Every call,
//! real or mock, flows through [`trace::TraceRecorder`] before returning so
//! a run can be replayed from the trace alone.
//!
//! The trait is intentionally narrow: three methods (`complete`,
//! `complete_structured`, `embed`) and a single error type. Vendor-specific
//! choices (model names, structured-output mechanism, retry policy) live
//! inside each provider impl, not in the trait.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod anthropic;
pub mod mock;
pub mod openai;
pub mod retry;
pub mod trace;

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    fn id(&self) -> ProviderId;

    async fn complete(&self, req: CompletionRequest) -> Result<Completion, LlmError>;

    async fn complete_structured(
        &self,
        req: StructuredRequest,
    ) -> Result<StructuredResponse, LlmError>;

    async fn embed(&self, req: EmbedRequest) -> Result<Embedding, LlmError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    Anthropic,
    OpenAi,
    Voyage,
    Mock,
}

impl ProviderId {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderId::Anthropic => "anthropic",
            ProviderId::OpenAi => "openai",
            ProviderId::Voyage => "voyage",
            ProviderId::Mock => "mock",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub system: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completion {
    pub content: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub latency_ms: u64,
    pub provider_request_id: Option<String>,
}

/// A request whose response must conform to a named JSON schema. The schema
/// itself is a `serde_json::Value` so this layer doesn't dictate how each
/// provider enforces it — Anthropic uses tool-use, OpenAI uses tool calls or
/// JSON mode, the mock just trusts its fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub system: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub schema_name: String,
    pub schema: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredResponse {
    pub value: serde_json::Value,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub latency_ms: u64,
    pub provider_request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbedRequest {
    pub model: String,
    pub input: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    pub vectors: Vec<Vec<f32>>,
    pub tokens: u32,
    pub latency_ms: u64,
}

#[derive(Debug, Error)]
pub enum LlmError {
    /// Network blip, 5xx, or other condition that should be retried.
    #[error("transient: {0}")]
    Transient(String),

    /// Bad input, schema-violating response, missing fixture, etc. Do not retry.
    #[error("permanent: {0}")]
    Permanent(String),

    /// 429 with a server-suggested or backoff-derived wait.
    #[error("rate limit: retry after {0:?}")]
    RateLimit(Duration),

    /// 401/403 — credential failure.
    #[error("auth: {0}")]
    Auth(String),

    /// Prompt + expected output won't fit the model's context window.
    #[error("context length: needed {needed} tokens, max {max}")]
    ContextLength { needed: u32, max: u32 },

    /// Structured response failed schema validation.
    #[error("schema mismatch: {0}")]
    Schema(String),
}

/// Compute a deterministic SHA-256 over a request. The hash is the lookup
/// key the mock provider uses and the identifier the trace recorder logs.
/// Stable across runs because requests serialize through serde with field
/// ordering preserved.
pub fn request_sha<T: Serialize>(req: &T) -> [u8; 32] {
    use sha2::Digest;
    let canonical = serde_json::to_vec(req).expect("serde always serializes request types");
    let mut h = sha2::Sha256::new();
    h.update(&canonical);
    h.finalize().into()
}

pub fn sha_hex(sha: &[u8; 32]) -> String {
    hex::encode(sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_sha_is_stable() {
        let req = CompletionRequest {
            model: "claude-opus-4-7".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "hi".to_string(),
            }],
            system: None,
            temperature: 0.0,
            max_tokens: 100,
        };
        let a = request_sha(&req);
        let b = request_sha(&req);
        assert_eq!(a, b, "same request must hash identically");
    }

    #[test]
    fn request_sha_changes_with_content() {
        let mut req = CompletionRequest {
            model: "claude-opus-4-7".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "hi".to_string(),
            }],
            system: None,
            temperature: 0.0,
            max_tokens: 100,
        };
        let a = request_sha(&req);
        req.messages[0].content = "different".to_string();
        let b = request_sha(&req);
        assert_ne!(a, b, "different content must produce a different hash");
    }

    #[test]
    fn provider_id_str_is_stable() {
        assert_eq!(ProviderId::Anthropic.as_str(), "anthropic");
        assert_eq!(ProviderId::OpenAi.as_str(), "openai");
        assert_eq!(ProviderId::Voyage.as_str(), "voyage");
        assert_eq!(ProviderId::Mock.as_str(), "mock");
    }
}
