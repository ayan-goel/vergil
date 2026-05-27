//! Retry primitive shared across provider impls.
//!
//! Three attempts, exponential backoff (200ms base, x2 per attempt), jitter
//! in [0, 250ms). Only [`LlmError::Transient`] and [`LlmError::RateLimit`]
//! are retried; everything else (auth, permanent, schema, context length)
//! is treated as terminal because retrying would either burn budget or
//! produce the same failure.

use std::future::Future;
use std::time::Duration;

use rand::Rng;

use crate::LlmError;

const MAX_ATTEMPTS: u32 = 3;
const BASE_DELAY: Duration = Duration::from_millis(200);
const JITTER_MAX_MS: u64 = 250;

/// Invoke `f` up to three times, sleeping per [`retry_delay`] between
/// attempts. Returns the last error if all attempts fail.
pub async fn with_retry<F, Fut, T>(mut f: F) -> Result<T, LlmError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, LlmError>>,
{
    let mut last: Option<LlmError> = None;
    for attempt in 0..MAX_ATTEMPTS {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                let delay = retry_delay(&e, attempt);
                let retriable = delay.is_some();
                last = Some(e);
                match delay {
                    Some(d) => tokio::time::sleep(d).await,
                    None => return Err(last.unwrap()),
                }
                if !retriable {
                    break;
                }
            }
        }
    }
    Err(last.unwrap_or_else(|| LlmError::Permanent("retry: no attempts made".into())))
}

/// Return the sleep duration before attempt `attempt+1`, or `None` if the
/// error class isn't retried. Pure function so it's testable.
pub fn retry_delay(err: &LlmError, attempt: u32) -> Option<Duration> {
    match err {
        LlmError::Transient(_) => Some(exp_with_jitter(attempt)),
        LlmError::RateLimit(d) => Some(*d + jitter()),
        _ => None,
    }
}

fn exp_with_jitter(attempt: u32) -> Duration {
    let scale = 1u32 << attempt; // 1, 2, 4
    BASE_DELAY.saturating_mul(scale) + jitter()
}

fn jitter() -> Duration {
    let ms = rand::thread_rng().gen_range(0..JITTER_MAX_MS);
    Duration::from_millis(ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn returns_first_success_without_retry() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_ref = calls.clone();
        let out: Result<u32, LlmError> = with_retry(move || {
            let c = calls_ref.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(42)
            }
        })
        .await;
        assert_eq!(out.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_transient_up_to_three_times() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_ref = calls.clone();
        let out: Result<u32, LlmError> = with_retry(move || {
            let c = calls_ref.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(LlmError::Transient("nope".into()))
            }
        })
        .await;
        assert!(matches!(out, Err(LlmError::Transient(_))));
        assert_eq!(calls.load(Ordering::SeqCst), MAX_ATTEMPTS);
    }

    #[tokio::test]
    async fn does_not_retry_auth() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_ref = calls.clone();
        let out: Result<u32, LlmError> = with_retry(move || {
            let c = calls_ref.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(LlmError::Auth("bad key".into()))
            }
        })
        .await;
        assert!(matches!(out, Err(LlmError::Auth(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn does_not_retry_permanent_or_schema() {
        let factories: [fn() -> LlmError; 3] = [
            || LlmError::Permanent("perm".into()),
            || LlmError::Schema("bad".into()),
            || LlmError::ContextLength {
                needed: 100,
                max: 50,
            },
        ];
        for mk in factories {
            let calls = Arc::new(AtomicU32::new(0));
            let calls_ref = calls.clone();
            let _ = with_retry::<_, _, u32>(move || {
                let c = calls_ref.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(mk())
                }
            })
            .await;
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn retries_then_succeeds_on_third_try() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_ref = calls.clone();
        let out: Result<u32, LlmError> = with_retry(move || {
            let c = calls_ref.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(LlmError::Transient("flake".into()))
                } else {
                    Ok(7)
                }
            }
        })
        .await;
        assert_eq!(out.unwrap(), 7);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn retry_delay_classification_is_correct() {
        assert!(retry_delay(&LlmError::Transient("x".into()), 0).is_some());
        assert!(retry_delay(&LlmError::RateLimit(Duration::from_millis(500)), 0).is_some());
        assert!(retry_delay(&LlmError::Auth("x".into()), 0).is_none());
        assert!(retry_delay(&LlmError::Permanent("x".into()), 0).is_none());
        assert!(retry_delay(&LlmError::Schema("x".into()), 0).is_none());
        assert!(retry_delay(&LlmError::ContextLength { needed: 1, max: 0 }, 0).is_none());
    }
}
