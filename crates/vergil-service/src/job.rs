//! Job model — Phase 4 Slice C2.
//!
//! The struct shape V2's postgres schema mirrors. Phase 4 ships an
//! in-memory implementation of [`JobStore`]; V2 swaps in a real backend.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Opaque per-job identifier. UUIDv4 today; V2 can swap to any unique
/// scheme as long as it serializes as a string.
pub type JobId = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Body customers POST to `/v1/jobs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequest {
    pub contract_source: String,
    pub intent: String,
    #[serde(default)]
    pub properties_yaml: Option<String>,
    #[serde(default)]
    pub cost_budget_usd: Option<f64>,
    #[serde(default)]
    pub wall_clock_budget_seconds: Option<u64>,
}

/// Persisted job record. Mirrors the OpenAPI `Job` schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub status: JobStatus,
    pub submitted_at: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub failure_reason: Option<String>,
}

/// proof.json blob payload returned by `/v1/jobs/{id}/result`. Stored
/// as a serde_json::Value so vergil-service stays decoupled from
/// vergil-proof's exact schema version (V2 might serve historical
/// schemas too).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub proof: serde_json::Value,
}

impl Job {
    /// Construct a freshly-submitted job with a UUIDv4 id.
    pub fn new_pending() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            status: JobStatus::Pending,
            submitted_at: chrono::Utc::now().to_rfc3339(),
            started_at: None,
            completed_at: None,
            cost_usd: None,
            failure_reason: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pending_assigns_uuid() {
        let j = Job::new_pending();
        assert!(matches!(j.status, JobStatus::Pending));
        assert_eq!(j.id.len(), 36, "uuid-v4 is 36 chars");
        assert!(j.completed_at.is_none());
        assert!(j.cost_usd.is_none());
    }

    #[test]
    fn job_request_round_trips_through_json() {
        let req = JobRequest {
            contract_source: "contract C {}".into(),
            intent: "no overflow".into(),
            properties_yaml: None,
            cost_budget_usd: Some(5.0),
            wall_clock_budget_seconds: Some(600),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: JobRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.contract_source, "contract C {}");
        assert_eq!(back.cost_budget_usd, Some(5.0));
    }
}
