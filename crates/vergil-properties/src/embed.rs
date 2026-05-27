//! Embedding provider abstraction. Production uses Voyage-3; tests use
//! [`MockEmbedder`] with deterministic hash-derived vectors. OpenAI's
//! `text-embedding-3-large` is the documented fallback if Voyage is down
//! or unreachable; a thin OpenAI impl can land in Phase 3 if needed.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbedError {
    #[error("voyage api: {0}")]
    Api(String),
    #[error("voyage transport: {0}")]
    Transport(String),
    #[error("voyage response shape: {0}")]
    Schema(String),
}

#[async_trait]
pub trait Embedder: Send + Sync {
    /// Identifier embedded in cached vectors to invalidate when the user
    /// switches providers (e.g. `"voyage-3"`, `"openai-text-embedding-3-large"`).
    fn id(&self) -> &str;
    /// Vector dimensionality the embedder produces. Used by the cache to
    /// reject mismatched cache entries.
    fn dim(&self) -> usize;
    /// Embed a batch of strings into vectors of [`Self::dim`] floats each.
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>;
}

/// Deterministic hash-derived embedder used by every unit test. Same
/// input → same vector across runs, processes, and machines.
pub struct MockEmbedder {
    id: String,
    dim: usize,
}

impl MockEmbedder {
    pub fn new(id: impl Into<String>, dim: usize) -> Self {
        Self { id: id.into(), dim }
    }
}

#[async_trait]
impl Embedder for MockEmbedder {
    fn id(&self) -> &str {
        &self.id
    }
    fn dim(&self) -> usize {
        self.dim
    }
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(inputs.iter().map(|s| hash_vector(s, self.dim)).collect())
    }
}

fn hash_vector(s: &str, dim: usize) -> Vec<f32> {
    use sha2::Digest;
    let mut out = Vec::with_capacity(dim);
    let mut counter: u32 = 0;
    while out.len() < dim {
        let mut h = sha2::Sha256::new();
        h.update(s.as_bytes());
        h.update(counter.to_le_bytes());
        let digest = h.finalize();
        for chunk in digest.chunks(4) {
            if out.len() >= dim {
                break;
            }
            let bytes: [u8; 4] = chunk.try_into().unwrap_or([0; 4]);
            // Map [0, u32::MAX] to (-1.0, 1.0). Deterministic.
            let v = (u32::from_le_bytes(bytes) as f32 / u32::MAX as f32) * 2.0 - 1.0;
            out.push(v);
        }
        counter += 1;
    }
    // Normalize to unit length so cosine similarity == dot product.
    let norm = out.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut out {
            *v /= norm;
        }
    }
    out
}

pub struct VoyageEmbedder {
    api_key: String,
    base_url: String,
    model: String,
    dim: usize,
    client: reqwest::Client,
}

impl VoyageEmbedder {
    pub fn new(api_key: impl Into<String>) -> Self {
        // voyage-3 produces 1024-dim vectors by default.
        Self::with_model(api_key, "voyage-3", 1024)
    }

    pub fn with_model(api_key: impl Into<String>, model: impl Into<String>, dim: usize) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://api.voyageai.com/v1".to_string(),
            model: model.into(),
            dim,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

#[derive(Serialize)]
struct VoyageRequest<'a> {
    input: &'a [String],
    model: &'a str,
}

#[derive(Deserialize)]
struct VoyageResponse {
    data: Vec<VoyageDatum>,
}

#[derive(Deserialize)]
struct VoyageDatum {
    embedding: Vec<f32>,
}

#[async_trait]
impl Embedder for VoyageEmbedder {
    fn id(&self) -> &str {
        &self.model
    }
    fn dim(&self) -> usize {
        self.dim
    }
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        let body = VoyageRequest {
            input: inputs,
            model: &self.model,
        };
        let url = format!("{}/embeddings", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| EmbedError::Transport(format!("{e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| EmbedError::Transport(format!("{e}")))?;
        if !status.is_success() {
            return Err(EmbedError::Api(format!("{status}: {text}")));
        }
        let parsed: VoyageResponse =
            serde_json::from_str(&text).map_err(|e| EmbedError::Schema(format!("{e}")))?;
        Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_embedder_is_deterministic() {
        let e = MockEmbedder::new("mock-32", 32);
        let v1 = e
            .embed(&["balance preservation".to_string()])
            .await
            .unwrap();
        let v2 = e
            .embed(&["balance preservation".to_string()])
            .await
            .unwrap();
        assert_eq!(v1, v2);
        assert_eq!(v1[0].len(), 32);
    }

    #[tokio::test]
    async fn mock_embedder_different_inputs_different_vectors() {
        let e = MockEmbedder::new("mock-32", 32);
        let v = e
            .embed(&["alpha".to_string(), "beta".to_string()])
            .await
            .unwrap();
        assert_eq!(v.len(), 2);
        assert_ne!(v[0], v[1]);
    }

    #[tokio::test]
    async fn mock_embedder_vectors_are_unit_length() {
        let e = MockEmbedder::new("mock-32", 32);
        let v = e.embed(&["x".to_string()]).await.unwrap();
        let norm: f32 = v[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "norm = {norm}");
    }
}
