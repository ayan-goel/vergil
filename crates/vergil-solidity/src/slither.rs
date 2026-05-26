//! Slither wrapper. Slither is used for structural information
//! (detectors, call graph) — **not** for storage layout, which goes
//! through solc (see [`crate::storage`]).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlitherResult {
    Ok(SlitherReport),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SlitherReport {
    pub detectors: Vec<Detector>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Detector {
    pub check: String,
    pub impact: String,
    pub confidence: String,
    pub description: String,
}

/// Parse Slither `--json -` output. Slither's top-level JSON is
/// `{"success": bool, "error": null|str, "results": {"detectors": [...]}}`.
pub fn parse_json(raw: &str) -> SlitherResult {
    #[derive(Deserialize)]
    struct Top {
        success: bool,
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        results: SlitherResults,
    }
    #[derive(Default, Deserialize)]
    struct SlitherResults {
        #[serde(default)]
        detectors: Vec<RawDetector>,
    }
    #[derive(Deserialize)]
    struct RawDetector {
        check: String,
        impact: String,
        confidence: String,
        #[serde(default)]
        description: String,
    }

    let top: Top = match serde_json::from_str(raw) {
        Ok(t) => t,
        Err(e) => return SlitherResult::Error(format!("invalid slither JSON: {e}")),
    };
    if !top.success {
        return SlitherResult::Error(top.error.unwrap_or_else(|| "slither failed".to_string()));
    }
    let detectors = top
        .results
        .detectors
        .into_iter()
        .map(|d| Detector {
            check: d.check,
            impact: d.impact,
            confidence: d.confidence,
            description: d.description,
        })
        .collect();
    SlitherResult::Ok(SlitherReport { detectors })
}

#[derive(Debug, Clone)]
pub struct SlitherRun {
    pub source: PathBuf,
    pub wall_clock_budget: Duration,
}

impl SlitherRun {
    pub fn new(source: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            wall_clock_budget: Duration::from_secs(60),
        }
    }

    pub fn with_wall_clock(mut self, b: Duration) -> Self {
        self.wall_clock_budget = b;
        self
    }
}

pub async fn run(cfg: &SlitherRun) -> SlitherResult {
    if !cfg.source.exists() {
        return SlitherResult::Error(format!("source not found: {}", cfg.source.display()));
    }
    let mut cmd = Command::new("slither");
    cmd.arg(&cfg.source)
        .arg("--json")
        .arg("-")
        .kill_on_drop(true);

    let result = timeout(cfg.wall_clock_budget, cmd.output()).await;
    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            parse_json(&stdout)
        }
        Ok(Err(e)) => SlitherResult::Error(format!("failed to spawn slither: {e}")),
        Err(_) => SlitherResult::Error("slither wall-clock budget exceeded".to_string()),
    }
}

pub async fn run_simple(source: &Path, budget: Duration) -> SlitherResult {
    run(&SlitherRun::new(source.to_path_buf()).with_wall_clock(budget)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const ERC20: &str = include_str!("../tests/fixtures/slither/erc20.json");

    #[test]
    fn erc20_fixture_parses() {
        match parse_json(ERC20) {
            SlitherResult::Ok(report) => {
                assert!(!report.detectors.is_empty());
                let checks: Vec<&str> = report.detectors.iter().map(|d| d.check.as_str()).collect();
                assert!(
                    checks.contains(&"solc-version"),
                    "expected solc-version detector, got {checks:?}"
                );
            }
            SlitherResult::Error(e) => panic!("expected Ok, got Error({e})"),
        }
    }

    #[test]
    fn malformed_json_is_error() {
        assert!(matches!(parse_json("not json"), SlitherResult::Error(_)));
    }

    #[test]
    fn failed_run_is_error() {
        let r = parse_json(r#"{"success": false, "error": "boom"}"#);
        match r {
            SlitherResult::Error(msg) => assert!(msg.contains("boom")),
            other => panic!("expected Error, got {other:?}"),
        }
    }
}
