//! Auth primitives — Phase 4 Slice C4.
//!
//! Token-based auth trait + single-token stub implementation. V2 swaps
//! in real multi-tenant auth (per-customer API keys, role-based access)
//! against the same trait so middleware doesn't change.

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("missing Authorization header")]
    MissingHeader,
    #[error("malformed Authorization header (expected `Bearer <token>`)")]
    Malformed,
    #[error("token rejected")]
    Rejected,
}

/// Identity payload returned by [`AuthProvider::authenticate`].
/// V2 will extend this with role + tenant fields; Phase 4's single-
/// token impl always returns `"internal"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthIdentity {
    pub tenant_id: String,
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Inspect the raw `Authorization` header value (e.g. `"Bearer abc"`)
    /// and return the identity or a typed reason for rejection.
    async fn authenticate(&self, auth_header: Option<&str>) -> Result<AuthIdentity, AuthError>;
}

/// Single-token implementation. Reads the expected token at
/// construction (typically from `VERGIL_SERVICE_TOKEN`); every request
/// must present that same token.
#[derive(Debug, Clone)]
pub struct SingleTokenAuth {
    expected: String,
    tenant_id: String,
}

impl SingleTokenAuth {
    pub fn new(expected: String) -> Self {
        Self {
            expected,
            tenant_id: "internal".to_string(),
        }
    }

    pub fn with_tenant_id(mut self, tenant_id: String) -> Self {
        self.tenant_id = tenant_id;
        self
    }
}

#[async_trait]
impl AuthProvider for SingleTokenAuth {
    async fn authenticate(&self, auth_header: Option<&str>) -> Result<AuthIdentity, AuthError> {
        let header = auth_header.ok_or(AuthError::MissingHeader)?;
        let token = header.strip_prefix("Bearer ").ok_or(AuthError::Malformed)?;
        if token != self.expected {
            return Err(AuthError::Rejected);
        }
        Ok(AuthIdentity {
            tenant_id: self.tenant_id.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn single_token_accepts_the_exact_token() {
        let auth = SingleTokenAuth::new("secret".into());
        let id = auth.authenticate(Some("Bearer secret")).await.unwrap();
        assert_eq!(id.tenant_id, "internal");
    }

    #[tokio::test]
    async fn single_token_rejects_wrong_token() {
        let auth = SingleTokenAuth::new("secret".into());
        let err = auth.authenticate(Some("Bearer wrong")).await.unwrap_err();
        assert!(matches!(err, AuthError::Rejected));
    }

    #[tokio::test]
    async fn missing_header_returns_typed_error() {
        let auth = SingleTokenAuth::new("secret".into());
        let err = auth.authenticate(None).await.unwrap_err();
        assert!(matches!(err, AuthError::MissingHeader));
    }

    #[tokio::test]
    async fn malformed_header_returns_typed_error() {
        let auth = SingleTokenAuth::new("secret".into());
        let err = auth.authenticate(Some("Basic abc")).await.unwrap_err();
        assert!(matches!(err, AuthError::Malformed));
    }

    #[tokio::test]
    async fn custom_tenant_id_flows_through_identity() {
        let auth = SingleTokenAuth::new("t".into()).with_tenant_id("acme-corp".into());
        let id = auth.authenticate(Some("Bearer t")).await.unwrap();
        assert_eq!(id.tenant_id, "acme-corp");
    }
}
