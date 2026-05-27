//! [`LlmProvider`] backed by the `anthropic-ai-sdk` crate (Honda).
//!
//! Surface:
//!   * `complete` — non-streaming Messages API call, returning the
//!     concatenated text content blocks.
//!   * `complete_structured` — single-tool call with `tool_choice: tool`,
//!     returning the tool input as the structured value.
//!   * `embed` — `LlmError::Permanent`. Anthropic does not host
//!     embeddings; Slice 8 routes embeddings to Voyage / OpenAI.
//!
//! Every call goes through [`retry::with_retry`] (3 attempts, exponential
//! backoff with jitter, retries only `Transient` + `RateLimit`). If a
//! [`trace::TraceRecorder`] is attached, every attempt's outcome (success
//! or terminal failure) is appended to the trace.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anthropic_ai_sdk::client::AnthropicClient as SdkClient;
use anthropic_ai_sdk::types::message::{
    ContentBlock as SdkContentBlock, CreateMessageParams, CreateMessageResponse,
    Message as SdkMessage, MessageClient, MessageError, RequiredMessageParams, Role as SdkRole,
    Tool, ToolChoice,
};
use async_trait::async_trait;

use crate::retry::with_retry;
use crate::trace::{llm_call_event, LlmCallParams, TraceRecorder};
use crate::{
    request_sha, Completion, CompletionRequest, EmbedRequest, Embedding, LlmError, LlmProvider,
    ProviderId, Role, StructuredRequest, StructuredResponse,
};

const API_VERSION: &str = "2023-06-01";

#[derive(Clone)]
pub struct AnthropicClient {
    inner: Arc<SdkClient>,
    tracer: Option<TraceRecorder>,
}

impl AnthropicClient {
    pub fn new(api_key: impl Into<String>) -> Result<Self, LlmError> {
        let sdk = SdkClient::new::<MessageError>(api_key, API_VERSION)
            .map_err(|e| LlmError::Permanent(format!("anthropic sdk init: {e}")))?;
        Ok(Self {
            inner: Arc::new(sdk),
            tracer: None,
        })
    }

    pub fn with_base_url(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Result<Self, LlmError> {
        let sdk = SdkClient::builder(api_key, API_VERSION)
            .with_api_base_url(base_url)
            .build::<MessageError>()
            .map_err(|e| LlmError::Permanent(format!("anthropic sdk init: {e}")))?;
        Ok(Self {
            inner: Arc::new(sdk),
            tracer: None,
        })
    }

    pub fn with_tracer(mut self, tracer: TraceRecorder) -> Self {
        self.tracer = Some(tracer);
        self
    }

    async fn record(&self, params: LlmCallParams<'_>, prompt: &str, response: &str) {
        if let Some(rec) = &self.tracer {
            let event = llm_call_event(params);
            if let Err(e) = rec.record(event, prompt, response).await {
                tracing::warn!("trace write failed: {e}");
            }
        }
    }
}

fn role_to_sdk(role: Role) -> SdkRole {
    match role {
        Role::User => SdkRole::User,
        Role::Assistant => SdkRole::Assistant,
    }
}

fn build_message_params(req: &CompletionRequest) -> CreateMessageParams {
    let mut params = CreateMessageParams::new(RequiredMessageParams {
        model: req.model.clone(),
        messages: req
            .messages
            .iter()
            .map(|m| SdkMessage::new_text(role_to_sdk(m.role), m.content.clone()))
            .collect(),
        max_tokens: req.max_tokens,
    })
    .with_temperature(req.temperature);
    if let Some(sys) = &req.system {
        params = params.with_system(sys.clone());
    }
    params
}

fn build_structured_params(req: &StructuredRequest) -> CreateMessageParams {
    let tool = Tool {
        name: req.schema_name.clone(),
        description: Some(format!(
            "Return the answer as structured input to this tool. Schema: {}",
            req.schema_name
        )),
        input_schema: req.schema.clone(),
    };
    let mut params = CreateMessageParams::new(RequiredMessageParams {
        model: req.model.clone(),
        messages: req
            .messages
            .iter()
            .map(|m| SdkMessage::new_text(role_to_sdk(m.role), m.content.clone()))
            .collect(),
        max_tokens: req.max_tokens,
    })
    .with_temperature(req.temperature)
    .with_tools(vec![tool])
    .with_tool_choice(ToolChoice::Tool {
        name: req.schema_name.clone(),
    });
    if let Some(sys) = &req.system {
        params = params.with_system(sys.clone());
    }
    params
}

fn flatten_text(blocks: &[SdkContentBlock]) -> String {
    let mut out = String::new();
    for b in blocks {
        if let SdkContentBlock::Text { text } = b {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text);
        }
    }
    out
}

fn first_tool_use(blocks: &[SdkContentBlock]) -> Option<&serde_json::Value> {
    for b in blocks {
        if let SdkContentBlock::ToolUse { input, .. } = b {
            return Some(input);
        }
    }
    None
}

fn map_message_error(e: MessageError) -> LlmError {
    let msg = e.to_string();
    let lower = msg.to_ascii_lowercase();
    if lower.contains("401")
        || lower.contains("403")
        || lower.contains("authentication")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("invalid api key")
        || lower.contains("invalid x-api-key")
        || lower.contains("authentication_error")
        || lower.contains("permission_error")
    {
        return LlmError::Auth(msg);
    }
    if lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("overloaded")
        || lower.contains("too many requests")
    {
        // The SDK error string doesn't carry retry-after; default delay.
        return LlmError::RateLimit(Duration::from_millis(500));
    }
    if lower.contains("invalid_request_error")
        || lower.contains("400")
        || lower.contains("schema")
        || lower.contains("bad request")
    {
        return LlmError::Permanent(msg);
    }
    if lower.contains("500")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
        || lower.contains("timeout")
        || lower.contains("connection")
        || lower.contains("network")
        || lower.contains("eof")
        || lower.contains("api_error")
        || lower.contains("internal")
        || lower.contains("upstream")
        || lower.contains("service unavailable")
    {
        return LlmError::Transient(msg);
    }
    // Unknown shape — default to Transient so the call is retried once;
    // a true permanent error will hit MAX_ATTEMPTS and propagate unchanged.
    LlmError::Transient(msg)
}

fn body_for_trace<T: serde::Serialize>(req: &T) -> String {
    serde_json::to_string(req).unwrap_or_default()
}

fn response_for_trace(resp: &CreateMessageResponse) -> String {
    serde_json::to_string(resp).unwrap_or_default()
}

#[async_trait]
impl LlmProvider for AnthropicClient {
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    async fn complete(&self, req: CompletionRequest) -> Result<Completion, LlmError> {
        let req_sha = request_sha(&req);
        let prompt_body = body_for_trace(&req);
        let model = req.model.clone();
        let temperature = req.temperature;
        let sdk_params = build_message_params(&req);
        let started = Instant::now();
        let sdk_resp: CreateMessageResponse = with_retry(|| async {
            self.inner
                .create_message(Some(&sdk_params))
                .await
                .map_err(map_message_error)
        })
        .await?;
        let latency_ms = started.elapsed().as_millis() as u64;
        let content = flatten_text(&sdk_resp.content);
        let response_body = response_for_trace(&sdk_resp);
        let response_sha = request_sha(&sdk_resp.id);
        self.record(
            LlmCallParams {
                provider: ProviderId::Anthropic,
                model,
                temperature,
                request_sha: &req_sha,
                response_sha: &response_sha,
                tokens_in: sdk_resp.usage.input_tokens,
                tokens_out: sdk_resp.usage.output_tokens,
                latency_ms,
                provider_request_id: Some(sdk_resp.id.clone()),
            },
            &prompt_body,
            &response_body,
        )
        .await;
        Ok(Completion {
            content,
            tokens_in: sdk_resp.usage.input_tokens,
            tokens_out: sdk_resp.usage.output_tokens,
            latency_ms,
            provider_request_id: Some(sdk_resp.id),
        })
    }

    async fn complete_structured(
        &self,
        req: StructuredRequest,
    ) -> Result<StructuredResponse, LlmError> {
        let req_sha = request_sha(&req);
        let prompt_body = body_for_trace(&req);
        let model = req.model.clone();
        let temperature = req.temperature;
        let schema_name = req.schema_name.clone();
        let sdk_params = build_structured_params(&req);
        let started = Instant::now();
        let sdk_resp: CreateMessageResponse = with_retry(|| async {
            self.inner
                .create_message(Some(&sdk_params))
                .await
                .map_err(map_message_error)
        })
        .await?;
        let latency_ms = started.elapsed().as_millis() as u64;
        let value = first_tool_use(&sdk_resp.content).cloned().ok_or_else(|| {
            LlmError::Schema(format!(
                "structured response missing tool_use block for schema {schema_name}"
            ))
        })?;
        let response_body = response_for_trace(&sdk_resp);
        let response_sha = request_sha(&sdk_resp.id);
        self.record(
            LlmCallParams {
                provider: ProviderId::Anthropic,
                model,
                temperature,
                request_sha: &req_sha,
                response_sha: &response_sha,
                tokens_in: sdk_resp.usage.input_tokens,
                tokens_out: sdk_resp.usage.output_tokens,
                latency_ms,
                provider_request_id: Some(sdk_resp.id.clone()),
            },
            &prompt_body,
            &response_body,
        )
        .await;
        Ok(StructuredResponse {
            value,
            tokens_in: sdk_resp.usage.input_tokens,
            tokens_out: sdk_resp.usage.output_tokens,
            latency_ms,
            provider_request_id: Some(sdk_resp.id),
        })
    }

    async fn embed(&self, _req: EmbedRequest) -> Result<Embedding, LlmError> {
        Err(LlmError::Permanent(
            "embeddings unsupported on Anthropic; use Voyage or OpenAI".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_401_to_auth() {
        let err = MessageError::ApiError("401 Unauthorized: invalid x-api-key".into());
        assert!(matches!(map_message_error(err), LlmError::Auth(_)));
    }

    #[test]
    fn maps_429_to_rate_limit() {
        let err = MessageError::ApiError("429 Too Many Requests: rate limit exceeded".into());
        assert!(matches!(map_message_error(err), LlmError::RateLimit(_)));
    }

    #[test]
    fn maps_5xx_to_transient() {
        for code in ["500", "502", "503", "504"] {
            let err = MessageError::ApiError(format!("{code} Internal"));
            assert!(
                matches!(map_message_error(err), LlmError::Transient(_)),
                "{code}"
            );
        }
    }

    #[test]
    fn maps_400_invalid_request_to_permanent() {
        let err = MessageError::ApiError("400 invalid_request_error: bad model".into());
        assert!(matches!(map_message_error(err), LlmError::Permanent(_)));
    }

    #[test]
    fn flatten_text_concatenates_text_blocks_only() {
        let blocks = vec![
            SdkContentBlock::Text {
                text: "alpha".into(),
            },
            SdkContentBlock::ToolUse {
                id: "1".into(),
                name: "x".into(),
                input: serde_json::json!({}),
            },
            SdkContentBlock::Text {
                text: "beta".into(),
            },
        ];
        assert_eq!(flatten_text(&blocks), "alpha\nbeta");
    }

    #[test]
    fn first_tool_use_finds_input() {
        let blocks = vec![
            SdkContentBlock::Text {
                text: "preamble".into(),
            },
            SdkContentBlock::ToolUse {
                id: "id-1".into(),
                name: "Schema".into(),
                input: serde_json::json!({"k": "v"}),
            },
        ];
        let v = first_tool_use(&blocks).unwrap();
        assert_eq!(v["k"], "v");
    }
}
