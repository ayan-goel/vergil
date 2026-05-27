//! Append-only JSONL trace of every LLM call.
//!
//! Layout under the run directory:
//!
//! ```text
//! trace/
//!   run.jsonl              one TraceEvent per line, parseable independently
//!   prompts/<sha>.txt      raw prompt body (request hash → text)
//!   responses/<sha>.txt    raw response body (request hash → text)
//! ```
//!
//! Splitting bodies out of the JSONL keeps the index small and grep-able while
//! still preserving every byte sent and received. The recorder scrubs any
//! occurrence of known secrets (API key env values, captured at construction)
//! from both the bodies and the event payload before writing.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::{sha_hex, ProviderId};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TraceEvent {
    LlmCall {
        provider: ProviderId,
        model: String,
        temperature: f32,
        request_sha: String,
        response_sha: String,
        tokens_in: u32,
        tokens_out: u32,
        latency_ms: u64,
        provider_request_id: Option<String>,
    },
}

const REDACTED: &str = "<REDACTED>";
const ENV_KEYS: &[&str] = &[
    "VERGIL_ANTHROPIC_API_KEY",
    "VERGIL_OPENAI_API_KEY",
    "VOYAGE_API_KEY",
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
];

#[derive(Clone)]
pub struct TraceRecorder {
    dir: PathBuf,
    file: Arc<Mutex<fs::File>>,
    secrets: Arc<Vec<String>>,
}

impl TraceRecorder {
    /// Open `<dir>/trace/run.jsonl` for append. `secrets` is the full list of
    /// strings to redact from bodies and event payloads before writing.
    /// Production callers should pass [`default_env_secrets`] plus any
    /// dynamic tokens; tests can pass any vector.
    pub async fn open(dir: impl AsRef<Path>, secrets: Vec<String>) -> std::io::Result<Self> {
        let trace_dir = dir.as_ref().join("trace");
        fs::create_dir_all(trace_dir.join("prompts")).await?;
        fs::create_dir_all(trace_dir.join("responses")).await?;
        let path = trace_dir.join("run.jsonl");
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        let secrets: Vec<String> = secrets.into_iter().filter(|s| !s.is_empty()).collect();
        Ok(Self {
            dir: trace_dir,
            file: Arc::new(Mutex::new(file)),
            secrets: Arc::new(secrets),
        })
    }

    /// Append one event. Stores the associated prompt and response bodies,
    /// scrubbing secrets from both, then writes the JSONL line. The line
    /// itself is also scrubbed (defense in depth — a misuse that puts a key
    /// into `provider_request_id` won't leak).
    pub async fn record(
        &self,
        event: TraceEvent,
        prompt_body: &str,
        response_body: &str,
    ) -> std::io::Result<()> {
        let (req_sha, resp_sha) = match &event {
            TraceEvent::LlmCall {
                request_sha,
                response_sha,
                ..
            } => (request_sha.clone(), response_sha.clone()),
        };

        let prompt_clean = self.scrub(prompt_body);
        let response_clean = self.scrub(response_body);

        self.write_body("prompts", &req_sha, &prompt_clean).await?;
        self.write_body("responses", &resp_sha, &response_clean)
            .await?;

        let line = serde_json::to_string(&event).expect("trace event always serializes");
        let line = self.scrub(&line);
        let mut f = self.file.lock().await;
        f.write_all(line.as_bytes()).await?;
        f.write_all(b"\n").await?;
        f.flush().await?;
        Ok(())
    }

    async fn write_body(&self, sub: &str, sha: &str, body: &str) -> std::io::Result<()> {
        let path = self.dir.join(sub).join(format!("{sha}.txt"));
        fs::write(path, body).await
    }

    fn scrub(&self, s: &str) -> String {
        let mut out = s.to_string();
        for secret in self.secrets.iter() {
            if !secret.is_empty() && out.contains(secret) {
                out = out.replace(secret.as_str(), REDACTED);
            }
        }
        out
    }
}

/// Read the canonical set of API-key env vars and return their values.
/// Production callers pass this to [`TraceRecorder::open`] so the recorder
/// can scrub credentials that leaked into a prompt or response body.
pub fn default_env_secrets() -> Vec<String> {
    ENV_KEYS
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .filter(|v| !v.is_empty())
        .collect()
}

/// Inputs to [`llm_call_event`]. A struct rather than a long positional
/// signature so adding a field (e.g. cost_usd in Slice 13) doesn't ripple
/// through every caller.
pub struct LlmCallParams<'a> {
    pub provider: ProviderId,
    pub model: String,
    pub temperature: f32,
    pub request_sha: &'a [u8; 32],
    pub response_sha: &'a [u8; 32],
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub latency_ms: u64,
    pub provider_request_id: Option<String>,
}

/// Derive an LlmCall event from a request hash + completion stats. Every
/// provider impl writes the same shape so the trace stays uniform.
pub fn llm_call_event(p: LlmCallParams<'_>) -> TraceEvent {
    TraceEvent::LlmCall {
        provider: p.provider,
        model: p.model,
        temperature: p.temperature,
        request_sha: sha_hex(p.request_sha),
        response_sha: sha_hex(p.response_sha),
        tokens_in: p.tokens_in,
        tokens_out: p.tokens_out,
        latency_ms: p.latency_ms,
        provider_request_id: p.provider_request_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_sha(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn params<'a>(
        req: &'a [u8; 32],
        resp: &'a [u8; 32],
        tokens_in: u32,
        tokens_out: u32,
        provider_request_id: Option<String>,
    ) -> LlmCallParams<'a> {
        LlmCallParams {
            provider: ProviderId::Mock,
            model: "mock-1".to_string(),
            temperature: 0.0,
            request_sha: req,
            response_sha: resp,
            tokens_in,
            tokens_out,
            latency_ms: 0,
            provider_request_id,
        }
    }

    #[tokio::test]
    async fn jsonl_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let rec = TraceRecorder::open(tmp.path(), vec![]).await.unwrap();
        let req = fake_sha(1);
        let resp = fake_sha(2);
        let ev = llm_call_event(LlmCallParams {
            latency_ms: 5,
            ..params(&req, &resp, 10, 20, Some("req-1".to_string()))
        });
        rec.record(ev, "the prompt", "the response").await.unwrap();

        let jsonl = tokio::fs::read_to_string(tmp.path().join("trace/run.jsonl"))
            .await
            .unwrap();
        let line = jsonl.lines().next().expect("at least one event");
        let parsed: TraceEvent = serde_json::from_str(line).expect("valid JSON");
        match parsed {
            TraceEvent::LlmCall {
                provider,
                model,
                tokens_in,
                ..
            } => {
                assert_eq!(provider, ProviderId::Mock);
                assert_eq!(model, "mock-1");
                assert_eq!(tokens_in, 10);
            }
        }
    }

    #[tokio::test]
    async fn body_files_written_at_sha_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let rec = TraceRecorder::open(tmp.path(), vec![]).await.unwrap();
        let req = fake_sha(0xAB);
        let resp = fake_sha(0xCD);
        let ev = llm_call_event(params(&req, &resp, 0, 0, None));
        rec.record(ev, "prompt-body", "response-body")
            .await
            .unwrap();

        let p = tokio::fs::read_to_string(
            tmp.path()
                .join("trace/prompts")
                .join(format!("{}.txt", sha_hex(&req))),
        )
        .await
        .unwrap();
        let r = tokio::fs::read_to_string(
            tmp.path()
                .join("trace/responses")
                .join(format!("{}.txt", sha_hex(&resp))),
        )
        .await
        .unwrap();
        assert_eq!(p, "prompt-body");
        assert_eq!(r, "response-body");
    }

    #[tokio::test]
    async fn extra_secrets_are_redacted_in_bodies_and_event() {
        let tmp = tempfile::tempdir().unwrap();
        let secret = "sk-fake-1234567890abcdef".to_string();
        let rec = TraceRecorder::open(tmp.path(), vec![secret.clone()])
            .await
            .unwrap();
        let req = fake_sha(1);
        let resp = fake_sha(2);
        // Provider id field leak path: stash the secret in provider_request_id.
        let ev = llm_call_event(params(&req, &resp, 0, 0, Some(secret.clone())));
        rec.record(ev, &format!("authz: {secret}"), &format!("echo: {secret}"))
            .await
            .unwrap();

        let jsonl = tokio::fs::read_to_string(tmp.path().join("trace/run.jsonl"))
            .await
            .unwrap();
        assert!(!jsonl.contains(&secret), "jsonl leaked secret: {jsonl}");
        assert!(jsonl.contains(REDACTED));

        let p = tokio::fs::read_to_string(
            tmp.path()
                .join("trace/prompts")
                .join(format!("{}.txt", sha_hex(&req))),
        )
        .await
        .unwrap();
        let r = tokio::fs::read_to_string(
            tmp.path()
                .join("trace/responses")
                .join(format!("{}.txt", sha_hex(&resp))),
        )
        .await
        .unwrap();
        assert!(!p.contains(&secret));
        assert!(!r.contains(&secret));
        assert!(p.contains(REDACTED));
        assert!(r.contains(REDACTED));
    }

    #[test]
    fn default_env_secrets_returns_only_set_nonempty_vars() {
        // We can't assume any of the keys are set in the test env, but we
        // can assert the function returns Strings, not Options or empties.
        let secrets = default_env_secrets();
        for s in &secrets {
            assert!(!s.is_empty(), "default_env_secrets must filter empties");
        }
    }

    #[test]
    fn env_keys_list_covers_phase2_providers() {
        // Pin the canonical list so a careless edit doesn't drop a provider.
        assert!(ENV_KEYS.contains(&"VERGIL_ANTHROPIC_API_KEY"));
        assert!(ENV_KEYS.contains(&"VERGIL_OPENAI_API_KEY"));
        assert!(ENV_KEYS.contains(&"VOYAGE_API_KEY"));
    }
}
