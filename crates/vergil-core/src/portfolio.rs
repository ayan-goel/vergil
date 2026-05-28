//! Portfolio dispatch: run multiple verification backends concurrently
//! and report whichever resolves first with a definitive verdict.
//!
//! Semantics (per SPEC §5.3 / Phase 1):
//!   * Definitive verdicts (Verified, Counterexample) cancel the loser.
//!   * Two non-definitive verdicts (Unknown, Timeout) → aggregate Unknown
//!     that surfaces both backends' info so the user knows what was tried.
//!   * A backend Error is reported only if it loses the race (otherwise the
//!     winner's verdict is what matters); if both error, the report is Unknown
//!     with a combined error message.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tokio::task::JoinHandle;
use vergil_solidity::halmos::{self, HalmosResult};
use vergil_solidity::smtchecker::{self, SmtCheckerResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Backend {
    Halmos,
    SmtChecker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum Verdict {
    Verified {
        backend: Backend,
        wall_clock_ms: u64,
        /// SHA-256 of the SMT-LIB query dumped by the verifier when query
        /// capture is enabled (Halmos `--dump-smt-queries`, SMTChecker
        /// `--model-checker-print-query`). `None` when the backend was run
        /// without query capture, or when the backend resolved the property
        /// without needing solver invocation. Phase 4 uses this for SMT
        /// re-dispatch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        smt_query_sha256: Option<String>,
    },
    Counterexample {
        backend: Backend,
        property: String,
        message: String,
        wall_clock_ms: u64,
    },
    Unknown {
        backends: Vec<BackendOutcome>,
    },
    Error {
        backends: Vec<BackendOutcome>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackendOutcome {
    pub backend: Backend,
    pub state: BackendState,
    pub detail: String,
    pub wall_clock_ms: u64,
    /// Populated when the backend dumped its SMT-LIB query and the hasher
    /// produced a digest. Threaded into [`Verdict::Verified::smt_query_sha256`]
    /// for the winning Verified outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smt_query_sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendState {
    Verified,
    Counterexample,
    Unknown,
    Timeout,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortfolioResult {
    pub property: String,
    pub verdict: Verdict,
}

/// Configuration for a portfolio run.
#[derive(Debug, Clone)]
pub struct PortfolioConfig {
    /// Foundry project directory (contains `foundry.toml`).
    pub project: PathBuf,
    /// `check_*` function name Halmos will verify.
    pub property: String,
    /// Source file passed to SMTChecker.
    pub smtchecker_source: PathBuf,
    /// Per-backend wall-clock budget.
    pub budget: Duration,
    /// When `true`, enable Halmos `--dump-smt-queries` and (Slice 3)
    /// SMTChecker `--model-checker-print-query` so the winning Verdict
    /// carries `smt_query_sha256`. Off by default for backward compat;
    /// the `vergil verify --intent` path turns it on.
    pub capture_smt_queries: bool,
    /// Phase 4 Slice A2: where to persist captured .smt2 files
    /// (named by their content SHA-256) so `vergil prove` can
    /// re-dispatch them later. Convention: `<project>/vergil-out/smt/`.
    /// When `None` but `capture_smt_queries` is on, the dump dir is
    /// hashed but not persisted.
    pub smt_persist_dir: Option<PathBuf>,
}

impl PortfolioConfig {
    /// Convenience constructor: enable SMT query capture + persistence,
    /// matching what the intent flow needs for `proof.json` population
    /// AND `vergil prove` re-dispatch.
    pub fn with_smt_capture(mut self) -> Self {
        self.capture_smt_queries = true;
        if self.smt_persist_dir.is_none() {
            self.smt_persist_dir = Some(self.project.join("vergil-out").join("smt"));
        }
        self
    }
}

/// Dispatch Halmos and SMTChecker concurrently; first definitive verdict wins.
pub async fn dispatch(cfg: PortfolioConfig) -> PortfolioResult {
    let property = cfg.property.clone();

    let halmos_project = cfg.project.clone();
    let halmos_property = cfg.property.clone();
    let halmos_budget = cfg.budget;
    let halmos_capture = cfg.capture_smt_queries;
    let halmos_persist = cfg.smt_persist_dir.clone();
    let halmos_task: JoinHandle<HalmosResult> = tokio::spawn(async move {
        if halmos_capture {
            let mut run_cfg = halmos::HalmosRun::new(halmos_project, halmos_property)
                .with_wall_clock(halmos_budget)
                .with_dump_smt2(true);
            if let Some(persist) = halmos_persist {
                run_cfg = run_cfg.with_smt_persist_directory(persist);
            }
            halmos::run(&run_cfg).await
        } else {
            halmos::run_simple(&halmos_project, &halmos_property, halmos_budget).await
        }
    });

    let smt_source = cfg.smtchecker_source.clone();
    let smt_budget = cfg.budget;
    let smt_capture = cfg.capture_smt_queries;
    let smt_task: JoinHandle<SmtCheckerResult> = tokio::spawn(async move {
        let dummy_project = smt_source
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| Path::new(".").to_path_buf());
        if smt_capture {
            let cfg = smtchecker::SmtCheckerRun::new(dummy_project, smt_source)
                .with_wall_clock(smt_budget)
                .with_print_query(true);
            smtchecker::run(&cfg).await
        } else {
            smtchecker::run_simple(&dummy_project, &smt_source, smt_budget).await
        }
    });

    let mut halmos_outcome: Option<BackendOutcome> = None;
    let mut smt_outcome: Option<BackendOutcome> = None;
    let mut definitive: Option<Verdict> = None;
    let mut pending_halmos = Some(halmos_task);
    let mut pending_smt = Some(smt_task);

    while pending_halmos.is_some() || pending_smt.is_some() {
        tokio::select! {
            biased;

            res = async {
                pending_halmos.as_mut().unwrap().await
            }, if pending_halmos.is_some() => {
                pending_halmos = None;
                let outcome = halmos_to_outcome(res);
                if let Some(v) = outcome.try_definitive(&property) {
                    definitive = Some(v);
                    if let Some(h) = pending_smt.take() {
                        h.abort();
                    }
                    halmos_outcome = Some(outcome);
                    break;
                }
                halmos_outcome = Some(outcome);
            }

            res = async {
                pending_smt.as_mut().unwrap().await
            }, if pending_smt.is_some() => {
                pending_smt = None;
                let outcome = smt_to_outcome(res);
                if let Some(v) = outcome.try_definitive(&property) {
                    definitive = Some(v);
                    if let Some(h) = pending_halmos.take() {
                        h.abort();
                    }
                    smt_outcome = Some(outcome);
                    break;
                }
                smt_outcome = Some(outcome);
            }
        }
    }

    let verdict = match definitive {
        Some(v) => v,
        None => {
            // Both backends finished without a definitive verdict (or with errors).
            let mut all = Vec::new();
            if let Some(o) = halmos_outcome.clone() {
                all.push(o);
            }
            if let Some(o) = smt_outcome.clone() {
                all.push(o);
            }
            let only_errors =
                !all.is_empty() && all.iter().all(|o| matches!(o.state, BackendState::Error));
            if only_errors {
                Verdict::Error { backends: all }
            } else {
                Verdict::Unknown { backends: all }
            }
        }
    };

    PortfolioResult { property, verdict }
}

fn halmos_to_outcome(join_result: Result<HalmosResult, tokio::task::JoinError>) -> BackendOutcome {
    let result = match join_result {
        Ok(r) => r,
        Err(e) => {
            return BackendOutcome {
                backend: Backend::Halmos,
                state: BackendState::Error,
                detail: format!("halmos task panic: {e}"),
                wall_clock_ms: 0,
                smt_query_sha256: None,
            };
        }
    };
    match result {
        HalmosResult::Verified {
            property: _,
            paths: _,
            wall_clock_ms,
            smt_query_sha256,
        } => BackendOutcome {
            backend: Backend::Halmos,
            state: BackendState::Verified,
            detail: "verified".to_string(),
            wall_clock_ms,
            smt_query_sha256,
        },
        HalmosResult::Counterexample {
            trace,
            wall_clock_ms,
        } => BackendOutcome {
            backend: Backend::Halmos,
            state: BackendState::Counterexample,
            detail: format!("counterexample with {} inputs", trace.inputs.len()),
            wall_clock_ms,
            smt_query_sha256: None,
        },
        HalmosResult::Unknown {
            reason,
            wall_clock_ms,
            ..
        } => BackendOutcome {
            backend: Backend::Halmos,
            state: BackendState::Unknown,
            detail: reason,
            wall_clock_ms,
            smt_query_sha256: None,
        },
        HalmosResult::Timeout {
            property: _,
            wall_clock_ms,
        } => BackendOutcome {
            backend: Backend::Halmos,
            state: BackendState::Timeout,
            detail: "wall-clock timeout".to_string(),
            wall_clock_ms,
            smt_query_sha256: None,
        },
        HalmosResult::Error { stage, message } => BackendOutcome {
            backend: Backend::Halmos,
            state: BackendState::Error,
            detail: format!("{stage} error: {message}"),
            wall_clock_ms: 0,
            smt_query_sha256: None,
        },
    }
}

fn smt_to_outcome(join_result: Result<SmtCheckerResult, tokio::task::JoinError>) -> BackendOutcome {
    let result = match join_result {
        Ok(r) => r,
        Err(e) => {
            return BackendOutcome {
                backend: Backend::SmtChecker,
                state: BackendState::Error,
                detail: format!("smtchecker task panic: {e}"),
                wall_clock_ms: 0,
                smt_query_sha256: None,
            };
        }
    };
    match result {
        SmtCheckerResult::Verified {
            proved_safe_count,
            wall_clock_ms,
            smt_query_sha256,
        } => BackendOutcome {
            backend: Backend::SmtChecker,
            state: BackendState::Verified,
            detail: format!("{proved_safe_count} target(s) proved safe"),
            wall_clock_ms,
            smt_query_sha256,
        },
        SmtCheckerResult::Violation {
            message,
            wall_clock_ms,
            ..
        } => BackendOutcome {
            backend: Backend::SmtChecker,
            state: BackendState::Counterexample,
            detail: message,
            wall_clock_ms,
            smt_query_sha256: None,
        },
        SmtCheckerResult::Unknown {
            reason,
            wall_clock_ms,
        } => BackendOutcome {
            backend: Backend::SmtChecker,
            state: BackendState::Unknown,
            detail: reason,
            wall_clock_ms,
            smt_query_sha256: None,
        },
        SmtCheckerResult::Error { message } => BackendOutcome {
            backend: Backend::SmtChecker,
            state: BackendState::Error,
            detail: message,
            wall_clock_ms: 0,
            smt_query_sha256: None,
        },
    }
}

impl BackendOutcome {
    fn try_definitive(&self, property: &str) -> Option<Verdict> {
        match self.state {
            BackendState::Verified => Some(Verdict::Verified {
                backend: self.backend,
                wall_clock_ms: self.wall_clock_ms,
                smt_query_sha256: self.smt_query_sha256.clone(),
            }),
            BackendState::Counterexample => Some(Verdict::Counterexample {
                backend: self.backend,
                property: property.to_string(),
                message: self.detail.clone(),
                wall_clock_ms: self.wall_clock_ms,
            }),
            BackendState::Unknown | BackendState::Timeout | BackendState::Error => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_outcome_is_definitive() {
        let o = BackendOutcome {
            backend: Backend::Halmos,
            state: BackendState::Verified,
            detail: String::new(),
            wall_clock_ms: 100,
            smt_query_sha256: None,
        };
        match o.try_definitive("p") {
            Some(Verdict::Verified { backend, .. }) => assert_eq!(backend, Backend::Halmos),
            other => panic!("expected Verified, got {other:?}"),
        }
    }

    #[test]
    fn verified_threads_smt_query_sha_into_verdict() {
        let h = "a".repeat(64);
        let o = BackendOutcome {
            backend: Backend::Halmos,
            state: BackendState::Verified,
            detail: String::new(),
            wall_clock_ms: 1,
            smt_query_sha256: Some(h.clone()),
        };
        match o.try_definitive("p").unwrap() {
            Verdict::Verified {
                smt_query_sha256, ..
            } => assert_eq!(smt_query_sha256, Some(h)),
            other => panic!("expected Verified, got {other:?}"),
        }
    }

    #[test]
    fn counterexample_outcome_is_definitive() {
        let o = BackendOutcome {
            backend: Backend::SmtChecker,
            state: BackendState::Counterexample,
            detail: "boom".into(),
            wall_clock_ms: 50,
            smt_query_sha256: None,
        };
        match o.try_definitive("check_x") {
            Some(Verdict::Counterexample {
                backend,
                property,
                message,
                ..
            }) => {
                assert_eq!(backend, Backend::SmtChecker);
                assert_eq!(property, "check_x");
                assert_eq!(message, "boom");
            }
            other => panic!("expected Counterexample, got {other:?}"),
        }
    }

    #[test]
    fn unknown_and_timeout_are_not_definitive() {
        for state in [
            BackendState::Unknown,
            BackendState::Timeout,
            BackendState::Error,
        ] {
            let o = BackendOutcome {
                backend: Backend::Halmos,
                state,
                detail: String::new(),
                wall_clock_ms: 0,
                smt_query_sha256: None,
            };
            assert!(o.try_definitive("p").is_none(), "{state:?}");
        }
    }
}
