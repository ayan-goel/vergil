//! [`JobStore`] trait + in-memory implementation. Phase 4 Slice C2.
//!
//! V2 swaps in a real postgres-backed implementation (the
//! `migrations/0001_jobs.sql` schema ships in Phase 4 too, unexecuted).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::job::{Job, JobId, JobResult, JobStatus};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("job not found: {0}")]
    NotFound(JobId),
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait JobStore: Send + Sync {
    async fn submit(&self, job: Job) -> Result<(), StoreError>;
    async fn get(&self, id: &JobId) -> Result<Job, StoreError>;
    async fn list(
        &self,
        status_filter: Option<JobStatus>,
        limit: usize,
    ) -> Result<Vec<Job>, StoreError>;
    async fn update_status(
        &self,
        id: &JobId,
        status: JobStatus,
        failure_reason: Option<String>,
    ) -> Result<(), StoreError>;
    async fn set_result(&self, id: &JobId, result: JobResult) -> Result<(), StoreError>;
    async fn get_result(&self, id: &JobId) -> Result<JobResult, StoreError>;
}

#[derive(Default)]
struct InMemoryInner {
    jobs: HashMap<JobId, Job>,
    results: HashMap<JobId, JobResult>,
}

#[derive(Default, Clone)]
pub struct InMemoryStore {
    inner: Arc<RwLock<InMemoryInner>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl JobStore for InMemoryStore {
    async fn submit(&self, job: Job) -> Result<(), StoreError> {
        let mut g = self.inner.write().await;
        g.jobs.insert(job.id.clone(), job);
        Ok(())
    }

    async fn get(&self, id: &JobId) -> Result<Job, StoreError> {
        let g = self.inner.read().await;
        g.jobs
            .get(id)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(id.clone()))
    }

    async fn list(
        &self,
        status_filter: Option<JobStatus>,
        limit: usize,
    ) -> Result<Vec<Job>, StoreError> {
        let g = self.inner.read().await;
        let mut out: Vec<Job> = g
            .jobs
            .values()
            .filter(|j| {
                status_filter
                    .as_ref()
                    .map(|s| s == &j.status)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        out.sort_by(|a, b| b.submitted_at.cmp(&a.submitted_at));
        out.truncate(limit);
        Ok(out)
    }

    async fn update_status(
        &self,
        id: &JobId,
        status: JobStatus,
        failure_reason: Option<String>,
    ) -> Result<(), StoreError> {
        let mut g = self.inner.write().await;
        let job = g
            .jobs
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound(id.clone()))?;
        match &status {
            JobStatus::Running if job.started_at.is_none() => {
                job.started_at = Some(chrono::Utc::now().to_rfc3339());
            }
            JobStatus::Completed | JobStatus::Failed if job.completed_at.is_none() => {
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
            }
            _ => {}
        }
        job.status = status;
        if let Some(reason) = failure_reason {
            job.failure_reason = Some(reason);
        }
        Ok(())
    }

    async fn set_result(&self, id: &JobId, result: JobResult) -> Result<(), StoreError> {
        let mut g = self.inner.write().await;
        if !g.jobs.contains_key(id) {
            return Err(StoreError::NotFound(id.clone()));
        }
        g.results.insert(id.clone(), result);
        Ok(())
    }

    async fn get_result(&self, id: &JobId) -> Result<JobResult, StoreError> {
        let g = self.inner.read().await;
        g.results
            .get(id)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn submit_and_get_round_trips() {
        let s = InMemoryStore::new();
        let job = Job::new_pending();
        let id = job.id.clone();
        s.submit(job).await.unwrap();
        let got = s.get(&id).await.unwrap();
        assert_eq!(got.id, id);
        assert!(matches!(got.status, JobStatus::Pending));
    }

    #[tokio::test]
    async fn get_unknown_id_is_not_found() {
        let s = InMemoryStore::new();
        let err = s.get(&"missing".to_string()).await.unwrap_err();
        assert!(matches!(err, StoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn update_status_to_running_stamps_started_at() {
        let s = InMemoryStore::new();
        let job = Job::new_pending();
        let id = job.id.clone();
        s.submit(job).await.unwrap();
        s.update_status(&id, JobStatus::Running, None)
            .await
            .unwrap();
        let got = s.get(&id).await.unwrap();
        assert!(matches!(got.status, JobStatus::Running));
        assert!(got.started_at.is_some());
        assert!(got.completed_at.is_none());
    }

    #[tokio::test]
    async fn update_status_to_completed_stamps_completed_at() {
        let s = InMemoryStore::new();
        let job = Job::new_pending();
        let id = job.id.clone();
        s.submit(job).await.unwrap();
        s.update_status(&id, JobStatus::Completed, None)
            .await
            .unwrap();
        let got = s.get(&id).await.unwrap();
        assert!(got.completed_at.is_some());
    }

    #[tokio::test]
    async fn list_filters_by_status_and_caps_limit() {
        let s = InMemoryStore::new();
        for _ in 0..3 {
            s.submit(Job::new_pending()).await.unwrap();
        }
        let one_running = Job::new_pending();
        let running_id = one_running.id.clone();
        s.submit(one_running).await.unwrap();
        s.update_status(&running_id, JobStatus::Running, None)
            .await
            .unwrap();

        let pending = s.list(Some(JobStatus::Pending), 10).await.unwrap();
        assert_eq!(pending.len(), 3);
        let running = s.list(Some(JobStatus::Running), 10).await.unwrap();
        assert_eq!(running.len(), 1);

        let limited = s.list(None, 2).await.unwrap();
        assert_eq!(limited.len(), 2);
    }

    #[tokio::test]
    async fn set_result_requires_existing_job() {
        let s = InMemoryStore::new();
        let err = s
            .set_result(
                &"missing".to_string(),
                JobResult {
                    proof: serde_json::json!({}),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::NotFound(_)));
    }
}
