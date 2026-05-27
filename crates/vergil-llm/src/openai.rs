//! [`LlmProvider`] backed by the `async-openai` crate.
//!
//! Surface:
//!   * `complete` — chat-completions call; returns the concatenated
//!     `choices[*].message.content` (effectively `choices[0]` for n=1).
//!   * `complete_structured` — single function-tool dispatch with
//!     `tool_choice: { type: "function", function: { name } }`; the
//!     model's tool call arguments (a JSON string per OpenAI's spec)
//!     are parsed and returned as the structured value.
//!   * `embed` — real vectors via the OpenAI embeddings endpoint.
//!
//! Retry + tracing semantics mirror [`anthropic::AnthropicClient`]:
//! three attempts with exponential backoff + jitter, retry only
//! `Transient` + `RateLimit`, full trace via [`TraceRecorder`] when one is
//! attached.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_openai::config::OpenAIConfig;
use async_openai::error::OpenAIError;
use async_openai::types::chat::{
    ChatCompletionMessageToolCalls, ChatCompletionNamedToolChoice,
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    ChatCompletionTool, ChatCompletionToolChoiceOption, ChatCompletionTools,
    CreateChatCompletionRequestArgs, CreateChatCompletionResponse, FunctionName,
    FunctionObjectArgs,
};
use async_openai::types::embeddings::{CreateEmbeddingRequestArgs, EmbeddingInput};
use async_openai::Client;
use async_trait::async_trait;

use crate::retry::with_retry;
use crate::trace::{llm_call_event, LlmCallParams, TraceRecorder};
use crate::{
    request_sha, Completion, CompletionRequest, EmbedRequest, Embedding, LlmError, LlmProvider,
    ProviderId, Role, StructuredRequest, StructuredResponse,
};

#[derive(Clone)]
pub struct OpenAiClient {
    inner: Arc<Client<OpenAIConfig>>,
    tracer: Option<TraceRecorder>,
}

impl OpenAiClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        let cfg = OpenAIConfig::new().with_api_key(api_key);
        Self {
            inner: Arc::new(Client::with_config(cfg)),
            tracer: None,
        }
    }

    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        let cfg = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(base_url);
        Self {
            inner: Arc::new(Client::with_config(cfg)),
            tracer: None,
        }
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

fn to_openai_messages(
    req_messages: &[crate::Message],
    system: Option<&str>,
) -> Result<Vec<ChatCompletionRequestMessage>, LlmError> {
    let mut out: Vec<ChatCompletionRequestMessage> = Vec::with_capacity(req_messages.len() + 1);
    if let Some(sys) = system {
        let m = ChatCompletionRequestSystemMessageArgs::default()
            .content(sys)
            .build()
            .map_err(|e| LlmError::Permanent(format!("build system message: {e}")))?;
        out.push(m.into());
    }
    for m in req_messages {
        let built: ChatCompletionRequestMessage = match m.role {
            Role::User => ChatCompletionRequestUserMessageArgs::default()
                .content(m.content.clone())
                .build()
                .map_err(|e| LlmError::Permanent(format!("build user message: {e}")))?
                .into(),
            Role::Assistant => ChatCompletionRequestAssistantMessageArgs::default()
                .content(m.content.clone())
                .build()
                .map_err(|e| LlmError::Permanent(format!("build assistant message: {e}")))?
                .into(),
        };
        out.push(built);
    }
    Ok(out)
}

fn map_openai_error(e: OpenAIError) -> LlmError {
    match &e {
        OpenAIError::ApiError(resp) => {
            let status = resp.status_code.as_u16();
            let msg = format!("{e}");
            match status {
                401 | 403 => LlmError::Auth(msg),
                429 => LlmError::RateLimit(Duration::from_millis(500)),
                400 | 404 | 422 => LlmError::Permanent(msg),
                // 5xx or anything unrecognized — treat as transient so the
                // retry helper gets one or two more shots before propagating.
                _ => LlmError::Transient(msg),
            }
        }
        OpenAIError::Reqwest(_) | OpenAIError::StreamError(_) => {
            LlmError::Transient(format!("{e}"))
        }
        OpenAIError::JSONDeserialize(_, _) => LlmError::Schema(format!("{e}")),
        OpenAIError::InvalidArgument(_) => LlmError::Permanent(format!("{e}")),
        _ => LlmError::Transient(format!("{e}")),
    }
}

fn flatten_choices(resp: &CreateChatCompletionResponse) -> String {
    let mut out = String::new();
    for c in &resp.choices {
        if let Some(text) = &c.message.content {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text);
        }
    }
    out
}

fn first_function_arguments_json(resp: &CreateChatCompletionResponse) -> Option<serde_json::Value> {
    let calls = resp.choices.first()?.message.tool_calls.as_ref()?;
    for call in calls {
        if let ChatCompletionMessageToolCalls::Function(f) = call {
            return serde_json::from_str::<serde_json::Value>(&f.function.arguments).ok();
        }
    }
    None
}

fn body_for_trace<T: serde::Serialize>(req: &T) -> String {
    serde_json::to_string(req).unwrap_or_default()
}

fn response_for_trace(resp: &CreateChatCompletionResponse) -> String {
    serde_json::to_string(resp).unwrap_or_default()
}

#[async_trait]
impl LlmProvider for OpenAiClient {
    fn id(&self) -> ProviderId {
        ProviderId::OpenAi
    }

    async fn complete(&self, req: CompletionRequest) -> Result<Completion, LlmError> {
        let req_sha = request_sha(&req);
        let prompt_body = body_for_trace(&req);
        let model = req.model.clone();
        let temperature = req.temperature;
        let messages = to_openai_messages(&req.messages, req.system.as_deref())?;
        let openai_req = CreateChatCompletionRequestArgs::default()
            .model(req.model.clone())
            .messages(messages)
            .temperature(req.temperature)
            .max_completion_tokens(req.max_tokens)
            .build()
            .map_err(|e| LlmError::Permanent(format!("build chat request: {e}")))?;

        let started = Instant::now();
        let resp: CreateChatCompletionResponse = with_retry(|| async {
            self.inner
                .chat()
                .create(openai_req.clone())
                .await
                .map_err(map_openai_error)
        })
        .await?;
        let latency_ms = started.elapsed().as_millis() as u64;
        let content = flatten_choices(&resp);
        let usage = resp.usage.clone().unwrap_or_default();
        let response_body = response_for_trace(&resp);
        let response_sha = request_sha(&resp.id);
        self.record(
            LlmCallParams {
                provider: ProviderId::OpenAi,
                model,
                temperature,
                request_sha: &req_sha,
                response_sha: &response_sha,
                tokens_in: usage.prompt_tokens,
                tokens_out: usage.completion_tokens,
                latency_ms,
                provider_request_id: Some(resp.id.clone()),
            },
            &prompt_body,
            &response_body,
        )
        .await;
        Ok(Completion {
            content,
            tokens_in: usage.prompt_tokens,
            tokens_out: usage.completion_tokens,
            latency_ms,
            provider_request_id: Some(resp.id),
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
        let messages = to_openai_messages(&req.messages, req.system.as_deref())?;
        let function = FunctionObjectArgs::default()
            .name(&schema_name)
            .description(format!(
                "Return the answer as structured arguments to this function. Schema: {schema_name}"
            ))
            .parameters(req.schema.clone())
            .build()
            .map_err(|e| LlmError::Permanent(format!("build function object: {e}")))?;
        let tool = ChatCompletionTools::Function(ChatCompletionTool { function });
        let openai_req = CreateChatCompletionRequestArgs::default()
            .model(req.model.clone())
            .messages(messages)
            .temperature(req.temperature)
            .max_completion_tokens(req.max_tokens)
            .tools(vec![tool])
            .tool_choice(ChatCompletionToolChoiceOption::Function(
                ChatCompletionNamedToolChoice {
                    function: FunctionName {
                        name: schema_name.clone(),
                    },
                },
            ))
            .build()
            .map_err(|e| LlmError::Permanent(format!("build chat request: {e}")))?;

        let started = Instant::now();
        let resp: CreateChatCompletionResponse = with_retry(|| async {
            self.inner
                .chat()
                .create(openai_req.clone())
                .await
                .map_err(map_openai_error)
        })
        .await?;
        let latency_ms = started.elapsed().as_millis() as u64;
        let value = first_function_arguments_json(&resp).ok_or_else(|| {
            LlmError::Schema(format!(
                "structured response missing function-tool call for schema {schema_name}"
            ))
        })?;
        let usage = resp.usage.clone().unwrap_or_default();
        let response_body = response_for_trace(&resp);
        let response_sha = request_sha(&resp.id);
        self.record(
            LlmCallParams {
                provider: ProviderId::OpenAi,
                model,
                temperature,
                request_sha: &req_sha,
                response_sha: &response_sha,
                tokens_in: usage.prompt_tokens,
                tokens_out: usage.completion_tokens,
                latency_ms,
                provider_request_id: Some(resp.id.clone()),
            },
            &prompt_body,
            &response_body,
        )
        .await;
        Ok(StructuredResponse {
            value,
            tokens_in: usage.prompt_tokens,
            tokens_out: usage.completion_tokens,
            latency_ms,
            provider_request_id: Some(resp.id),
        })
    }

    async fn embed(&self, req: EmbedRequest) -> Result<Embedding, LlmError> {
        let req_sha = request_sha(&req);
        let prompt_body = body_for_trace(&req);
        let model = req.model.clone();
        let openai_req = CreateEmbeddingRequestArgs::default()
            .model(req.model.clone())
            .input(EmbeddingInput::StringArray(req.input.clone()))
            .build()
            .map_err(|e| LlmError::Permanent(format!("build embed request: {e}")))?;

        let started = Instant::now();
        let resp = with_retry(|| async {
            self.inner
                .embeddings()
                .create(openai_req.clone())
                .await
                .map_err(map_openai_error)
        })
        .await?;
        let latency_ms = started.elapsed().as_millis() as u64;
        let vectors: Vec<Vec<f32>> = resp.data.iter().map(|d| d.embedding.clone()).collect();
        let tokens = resp.usage.total_tokens;
        let response_body = serde_json::to_string(&resp).unwrap_or_default();
        let response_sha = request_sha(&model);
        self.record(
            LlmCallParams {
                provider: ProviderId::OpenAi,
                model,
                temperature: 0.0,
                request_sha: &req_sha,
                response_sha: &response_sha,
                tokens_in: resp.usage.prompt_tokens,
                tokens_out: 0,
                latency_ms,
                provider_request_id: None,
            },
            &prompt_body,
            &response_body,
        )
        .await;
        Ok(Embedding {
            vectors,
            tokens,
            latency_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_openai::error::{ApiError, ApiErrorResponse};
    use reqwest::StatusCode;

    fn api_err(status: u16) -> OpenAIError {
        OpenAIError::ApiError(ApiErrorResponse {
            status_code: StatusCode::from_u16(status).unwrap(),
            api_error: ApiError {
                message: format!("status {status}"),
                r#type: None,
                param: None,
                code: None,
            },
        })
    }

    #[test]
    fn maps_401_to_auth() {
        assert!(matches!(map_openai_error(api_err(401)), LlmError::Auth(_)));
    }

    #[test]
    fn maps_429_to_rate_limit() {
        assert!(matches!(
            map_openai_error(api_err(429)),
            LlmError::RateLimit(_)
        ));
    }

    #[test]
    fn maps_5xx_to_transient() {
        for code in [500u16, 502, 503, 504] {
            assert!(
                matches!(map_openai_error(api_err(code)), LlmError::Transient(_)),
                "{code}"
            );
        }
    }

    #[test]
    fn maps_400_to_permanent() {
        assert!(matches!(
            map_openai_error(api_err(400)),
            LlmError::Permanent(_)
        ));
    }
}
