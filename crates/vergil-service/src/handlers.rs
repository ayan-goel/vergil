//! Axum handlers — Phase 4 Slice C1.
//!
//! All endpoints return `501 Not Implemented` (with the right JSON
//! shape from the OpenAPI contract) until V2 plugs in the worker pool.
//! The endpoints DO go through auth + the job store interface so V2
//! sees the seams in action.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::auth::{AuthError, AuthIdentity};
use crate::job::{Job, JobRequest, JobStatus};
use crate::AppState;

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
    detail: String,
}

#[derive(Debug, Deserialize)]
pub struct ListJobsQuery {
    pub status: Option<JobStatus>,
    pub limit: Option<usize>,
    #[serde(default)]
    pub cursor: Option<String>,
}

async fn require_auth(state: &AppState, headers: &HeaderMap) -> Result<AuthIdentity, Response> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    match state.auth.authenticate(auth_header).await {
        Ok(id) => Ok(id),
        Err(e) => {
            let body = ApiError {
                error: match &e {
                    AuthError::MissingHeader => "missing_authorization".to_string(),
                    AuthError::Malformed => "malformed_authorization".to_string(),
                    AuthError::Rejected => "token_rejected".to_string(),
                },
                detail: format!("{e}"),
            };
            Err((StatusCode::UNAUTHORIZED, Json(body)).into_response())
        }
    }
}

fn not_implemented(message: &str) -> Response {
    let body = ApiError {
        error: "not_implemented".to_string(),
        detail: format!("{message} — V2 implements the bodies; Phase 4 ships the contract only"),
    };
    (StatusCode::NOT_IMPLEMENTED, Json(body)).into_response()
}

pub async fn submit_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_body): Json<JobRequest>,
) -> Response {
    if let Err(resp) = require_auth(&state, &headers).await {
        return resp;
    }
    // Phase 4: persist the freshly-submitted job (lets ops see the
    // request shape in the in-memory store) but do not actually queue
    // for verification. V2 hooks the worker pool here.
    let job = Job::new_pending();
    let id = job.id.clone();
    if let Err(e) = state.store.submit(job.clone()).await {
        let body = ApiError {
            error: "storage_error".to_string(),
            detail: format!("{e}"),
        };
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response();
    }
    let _ = id;
    not_implemented("submit_job: queue not wired")
}

pub async fn list_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListJobsQuery>,
) -> Response {
    if let Err(resp) = require_auth(&state, &headers).await {
        return resp;
    }
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    match state.store.list(q.status, limit).await {
        Ok(jobs) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "jobs": jobs,
                "next_cursor": serde_json::Value::Null,
            })),
        )
            .into_response(),
        Err(e) => {
            let body = ApiError {
                error: "storage_error".to_string(),
                detail: format!("{e}"),
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
        }
    }
}

pub async fn get_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(resp) = require_auth(&state, &headers).await {
        return resp;
    }
    match state.store.get(&id).await {
        Ok(job) => (StatusCode::OK, Json(job)).into_response(),
        Err(_) => {
            let body = ApiError {
                error: "not_found".to_string(),
                detail: format!("unknown job id: {id}"),
            };
            (StatusCode::NOT_FOUND, Json(body)).into_response()
        }
    }
}

pub async fn get_job_result(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(resp) = require_auth(&state, &headers).await {
        return resp;
    }
    match state.store.get_result(&id).await {
        Ok(result) => (StatusCode::OK, Json(result.proof)).into_response(),
        Err(_) => {
            let body = ApiError {
                error: "not_found".to_string(),
                detail: format!("no result for job id (yet): {id}"),
            };
            (StatusCode::NOT_FOUND, Json(body)).into_response()
        }
    }
}

pub async fn healthz() -> Response {
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"}))).into_response()
}

/// Prometheus text-format metrics endpoint. Stub for Phase 4 — emits
/// zero baselines for the stable counter names so V2's scraper can
/// connect on day one. V2 wires real increments through `state.metrics`.
pub async fn metrics(State(state): State<AppState>) -> Response {
    let body = state.metrics.render_text();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryStore;
    use crate::SingleTokenAuth;
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        AppState {
            store: Arc::new(InMemoryStore::new()),
            auth: Arc::new(SingleTokenAuth::new("dev-token".into())),
            metrics: Arc::new(crate::Metrics::new()),
        }
    }

    #[tokio::test]
    async fn healthz_returns_ok() {
        let app = crate::router(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn submit_without_auth_returns_401() {
        let app = crate::router(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/jobs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"contract_source":"contract C{}","intent":"i"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn submit_with_auth_returns_501_stub() {
        let app = crate::router(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/jobs")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer dev-token")
                    .body(Body::from(
                        r#"{"contract_source":"contract C{}","intent":"i"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn list_with_auth_returns_empty_page() {
        let app = crate::router(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/jobs")
                    .header("authorization", "Bearer dev-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn metrics_endpoint_returns_prometheus_text_with_zero_baselines() {
        let app = crate::router(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(ct.starts_with("text/plain"));
        let body = axum::body::to_bytes(resp.into_body(), 8 * 1024)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("# TYPE vergil_jobs_total counter"));
        assert!(text.contains("vergil_jobs_total 0"));
    }

    #[tokio::test]
    async fn get_unknown_job_returns_404() {
        let app = crate::router(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/jobs/non-existent")
                    .header("authorization", "Bearer dev-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
