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
}

/// Dispatch Halmos and SMTChecker concurrently; first definitive verdict wins.
pub async fn dispatch(cfg: PortfolioConfig) -> PortfolioResult {
    let property = cfg.property.clone();

    let halmos_project = cfg.project.clone();
    let halmos_property = cfg.property.clone();
    let halmos_budget = cfg.budget;
    let halmos_task: JoinHandle<HalmosResult> = tokio::spawn(async move {
        halmos::run_simple(&halmos_project, &halmos_property, halmos_budget).await
    });

    let smt_source = cfg.smtchecker_source.clone();
    let smt_budget = cfg.budget;
    let smt_task: JoinHandle<SmtCheckerResult> = tokio::spawn(async move {
        let dummy_project = smt_source
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| Path::new(".").to_path_buf());
        smtchecker::run_simple(&dummy_project, &smt_source, smt_budget).await
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
            };
        }
    };
    match result {
        HalmosResult::Verified {
            property: _,
            paths: _,
            wall_clock_ms,
        } => BackendOutcome {
            backend: Backend::Halmos,
            state: BackendState::Verified,
            detail: "verified".to_string(),
            wall_clock_ms,
        },
        HalmosResult::Counterexample {
            trace,
            wall_clock_ms,
        } => BackendOutcome {
            backend: Backend::Halmos,
            state: BackendState::Counterexample,
            detail: format!("counterexample with {} inputs", trace.inputs.len()),
            wall_clock_ms,
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
        },
        HalmosResult::Timeout {
            property: _,
            wall_clock_ms,
        } => BackendOutcome {
            backend: Backend::Halmos,
            state: BackendState::Timeout,
            detail: "wall-clock timeout".to_string(),
            wall_clock_ms,
        },
        HalmosResult::Error { stage, message } => BackendOutcome {
            backend: Backend::Halmos,
            state: BackendState::Error,
            detail: format!("{stage} error: {message}"),
            wall_clock_ms: 0,
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
            };
        }
    };
    match result {
        SmtCheckerResult::Verified {
            proved_safe_count,
            wall_clock_ms,
        } => BackendOutcome {
            backend: Backend::SmtChecker,
            state: BackendState::Verified,
            detail: format!("{proved_safe_count} target(s) proved safe"),
            wall_clock_ms,
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
        },
        SmtCheckerResult::Unknown {
            reason,
            wall_clock_ms,
        } => BackendOutcome {
            backend: Backend::SmtChecker,
            state: BackendState::Unknown,
            detail: reason,
            wall_clock_ms,
        },
        SmtCheckerResult::Error { message } => BackendOutcome {
            backend: Backend::SmtChecker,
            state: BackendState::Error,
            detail: message,
            wall_clock_ms: 0,
        },
    }
}

impl BackendOutcome {
    fn try_definitive(&self, property: &str) -> Option<Verdict> {
        match self.state {
            BackendState::Verified => Some(Verdict::Verified {
                backend: self.backend,
                wall_clock_ms: self.wall_clock_ms,
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
        };
        match o.try_definitive("p") {
            Some(Verdict::Verified { backend, .. }) => assert_eq!(backend, Backend::Halmos),
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
            };
            assert!(o.try_definitive("p").is_none(), "{state:?}");
        }
    }
}
