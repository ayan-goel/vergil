//! Vergil service skeleton — Phase 4 Slice C1 / C2 / C4.
//!
//! Ships the wire-format contract V2 will fill in. Endpoints return `501
//! not_implemented` until V2 plugs in a worker pool + persistent
//! storage. The job model + in-memory store (Slice C2) and auth trait
//! (Slice C4) live alongside so V2 has the seams locked.
//!
//! See `openapi.yaml` at the repo root for the full API contract.

pub mod auth;
pub mod handlers;
pub mod job;
pub mod store;

use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};

pub use auth::{AuthError, AuthProvider, SingleTokenAuth};
pub use job::{Job, JobId, JobRequest, JobResult, JobStatus};
pub use store::{InMemoryStore, JobStore};

/// Shared application state passed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn JobStore>,
    pub auth: Arc<dyn AuthProvider>,
}

/// Build the axum Router for the v1 API. Production callers wrap this
/// with whatever middleware they need (tracing, rate-limit, CORS) and
/// hand to `axum::serve`.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route(
            "/v1/jobs",
            post(handlers::submit_job).get(handlers::list_jobs),
        )
        .route("/v1/jobs/{id}", get(handlers::get_job))
        .route("/v1/jobs/{id}/result", get(handlers::get_job_result))
        .route("/healthz", get(handlers::healthz))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_builds_with_default_state() {
        let state = AppState {
            store: Arc::new(InMemoryStore::new()),
            auth: Arc::new(SingleTokenAuth::new("dev-token".to_string())),
        };
        let _router = router(state);
        // If router() compiles, the type-level contract is satisfied.
    }
}
