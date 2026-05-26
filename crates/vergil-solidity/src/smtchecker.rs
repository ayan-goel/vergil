//! Wrapper around solc's built-in SMTChecker (CHC mode).
//!
//! We invoke `solc --standard-json` to get structured JSON output. The model
//! checker emits diagnostic objects under `errors[]`; we classify them into
//! Verified / Violation / Unknown / Error.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmtCheckerResult {
    /// All targets the model checker considered were proved safe.
    Verified {
        proved_safe_count: u32,
        wall_clock_ms: u64,
    },
    /// At least one property was violated. SMTChecker provides concrete inputs
    /// as part of the message; we surface the raw message verbatim for now.
    Violation {
        property_kind: PropertyKind,
        message: String,
        wall_clock_ms: u64,
    },
    /// SMTChecker could not decide (timeout, non-linear arithmetic, ...).
    Unknown { reason: String, wall_clock_ms: u64 },
    /// solc itself failed (compile error, invalid input).
    Error { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    Assertion,
    Overflow,
    Underflow,
    DivisionByZero,
    OutOfBounds,
    Other,
}

#[derive(Debug, Deserialize)]
struct SolcOutput {
    #[serde(default)]
    errors: Vec<SolcDiagnostic>,
}

#[derive(Debug, Deserialize)]
struct SolcDiagnostic {
    #[serde(default)]
    severity: String,
    #[serde(default)]
    message: String,
}

/// Parse solc standard-json output into a typed result.
pub fn parse_standard_json(json: &str) -> SmtCheckerResult {
    let output: SolcOutput = match serde_json::from_str(json) {
        Ok(o) => o,
        Err(e) => {
            return SmtCheckerResult::Error {
                message: format!("invalid solc JSON: {e}"),
            };
        }
    };

    // First pass: hard compiler errors are show-stoppers.
    for diag in &output.errors {
        if diag.severity == "error" && !is_chc_message(&diag.message) {
            return SmtCheckerResult::Error {
                message: diag.message.clone(),
            };
        }
    }

    // Second pass: CHC findings. A diagnostic is a real violation only when it
    // *isn't* a "check is safe" success message (which name-drops the property
    // kind, e.g. "Assertion violation check is safe!").
    for diag in &output.errors {
        if !is_chc_message(&diag.message) {
            continue;
        }
        if diag.message.contains("check is safe") || diag.message.contains("proved safe") {
            continue;
        }
        if diag.message.contains("Counterexample") || mentions_violation(&diag.message) {
            return SmtCheckerResult::Violation {
                property_kind: classify_property(&diag.message),
                message: diag.message.clone(),
                wall_clock_ms: 0,
            };
        }
        if diag.message.contains("could not be proved")
            || diag.message.contains("could not prove")
            || diag.message.contains("unknown")
        {
            return SmtCheckerResult::Unknown {
                reason: diag.message.clone(),
                wall_clock_ms: 0,
            };
        }
    }

    // Third pass: tally proved-safe info messages.
    //  - "N verification condition(s) proved safe!" → take N
    //  - "X check is safe!" (per-target, emitted when showProvedSafe=true) → count each
    let mut aggregated = 0u32;
    let mut individual = 0u32;
    for diag in &output.errors {
        if !is_chc_message(&diag.message) {
            continue;
        }
        if diag.message.contains("proved safe") {
            if let Some(n) = parse_count_from_message(&diag.message) {
                aggregated = aggregated.max(n);
            }
        } else if diag.message.contains("check is safe") {
            individual += 1;
        }
    }
    let proved_safe = aggregated.max(individual);

    SmtCheckerResult::Verified {
        proved_safe_count: proved_safe,
        wall_clock_ms: 0,
    }
}

fn is_chc_message(message: &str) -> bool {
    message.contains("CHC:") || message.contains("BMC:")
}

fn mentions_violation(message: &str) -> bool {
    message.contains("happens here")
        || message.contains("violated")
        || message.contains("Assertion violation")
}

fn classify_property(message: &str) -> PropertyKind {
    let lower = message.to_lowercase();
    if lower.contains("overflow") {
        PropertyKind::Overflow
    } else if lower.contains("underflow") {
        PropertyKind::Underflow
    } else if lower.contains("division by zero") {
        PropertyKind::DivisionByZero
    } else if lower.contains("out of bounds") || lower.contains("index out of") {
        PropertyKind::OutOfBounds
    } else if lower.contains("assert") {
        PropertyKind::Assertion
    } else {
        PropertyKind::Other
    }
}

fn parse_count_from_message(message: &str) -> Option<u32> {
    // "CHC: 1 verification condition(s) proved safe!"
    let after = message.split_once("CHC:")?.1.trim_start();
    let first_token = after.split_whitespace().next()?;
    first_token.parse().ok()
}

/// Configuration for a single SMTChecker run via `solc --standard-json`.
#[derive(Debug, Clone)]
pub struct SmtCheckerRun {
    pub project: PathBuf,
    pub source: PathBuf,
    pub wall_clock_budget: Duration,
    /// Per-query SMT timeout (ms). Forwarded to the model checker config.
    pub solver_timeout_ms: u64,
}

impl SmtCheckerRun {
    pub fn new(project: impl Into<PathBuf>, source: impl Into<PathBuf>) -> Self {
        Self {
            project: project.into(),
            source: source.into(),
            wall_clock_budget: Duration::from_secs(120),
            solver_timeout_ms: 30_000,
        }
    }

    pub fn with_wall_clock(mut self, budget: Duration) -> Self {
        self.wall_clock_budget = budget;
        self
    }

    pub fn with_solver_timeout_ms(mut self, ms: u64) -> Self {
        self.solver_timeout_ms = ms;
        self
    }
}

/// Spawn solc with `--standard-json`, parse output into a typed result.
pub async fn run(cfg: &SmtCheckerRun) -> SmtCheckerResult {
    if !cfg.source.is_file() {
        return SmtCheckerResult::Error {
            message: format!("source not found: {}", cfg.source.display()),
        };
    }
    let content = match std::fs::read_to_string(&cfg.source) {
        Ok(c) => c,
        Err(e) => {
            return SmtCheckerResult::Error {
                message: format!("could not read source {}: {e}", cfg.source.display()),
            };
        }
    };
    let source_key = cfg
        .source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "source.sol".to_string());

    let input = serde_json::json!({
        "language": "Solidity",
        "sources": { source_key.clone(): { "content": content } },
        "settings": {
            "modelChecker": {
                "engine": "chc",
                "targets": ["assert", "overflow", "underflow", "divByZero", "outOfBounds"],
                "timeout": cfg.solver_timeout_ms,
                "showProvedSafe": true,
            },
            "outputSelection": { "*": { "*": ["abi"] } }
        }
    });

    let mut cmd = Command::new("solc");
    cmd.arg("--standard-json")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .current_dir(&cfg.project)
        .kill_on_drop(true);

    let start = std::time::Instant::now();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return SmtCheckerResult::Error {
                message: format!("failed to spawn solc: {e}"),
            };
        }
    };

    let stdin_input = input.to_string();
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        if let Err(e) = stdin.write_all(stdin_input.as_bytes()).await {
            return SmtCheckerResult::Error {
                message: format!("failed writing to solc stdin: {e}"),
            };
        }
        drop(stdin);
    }

    let result = timeout(cfg.wall_clock_budget, child.wait_with_output()).await;
    let elapsed_ms = start.elapsed().as_millis() as u64;

    let output = match result {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            return SmtCheckerResult::Error {
                message: format!("solc wait failed: {e}"),
            };
        }
        Err(_) => {
            return SmtCheckerResult::Unknown {
                reason: "wall-clock budget exceeded".to_string(),
                wall_clock_ms: elapsed_ms,
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut parsed = parse_standard_json(&stdout);
    annotate_wall_clock(&mut parsed, elapsed_ms);
    parsed
}

/// Convenience wrapper preserving the `(project, source, budget)` shape.
pub async fn run_simple(project: &Path, source: &Path, budget: Duration) -> SmtCheckerResult {
    let cfg =
        SmtCheckerRun::new(project.to_path_buf(), source.to_path_buf()).with_wall_clock(budget);
    run(&cfg).await
}

fn annotate_wall_clock(r: &mut SmtCheckerResult, ms: u64) {
    match r {
        SmtCheckerResult::Verified { wall_clock_ms, .. } => *wall_clock_ms = ms,
        SmtCheckerResult::Violation { wall_clock_ms, .. } => *wall_clock_ms = ms,
        SmtCheckerResult::Unknown { wall_clock_ms, .. } => *wall_clock_ms = ms,
        SmtCheckerResult::Error { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERIFIED: &str = include_str!("../tests/fixtures/smtchecker/verified.json");
    const VIOLATION: &str = include_str!("../tests/fixtures/smtchecker/violation-overflow.json");
    const UNKNOWN: &str = include_str!("../tests/fixtures/smtchecker/unknown.json");

    #[test]
    fn verified_fixture_parses_as_verified() {
        let r = parse_standard_json(VERIFIED);
        match r {
            SmtCheckerResult::Verified {
                proved_safe_count, ..
            } => {
                assert!(
                    proved_safe_count >= 1,
                    "expected at least one proved-safe, got {proved_safe_count}"
                );
            }
            other => panic!("expected Verified, got {other:?}"),
        }
    }

    #[test]
    fn overflow_fixture_parses_as_violation() {
        let r = parse_standard_json(VIOLATION);
        match r {
            SmtCheckerResult::Violation {
                property_kind,
                message,
                ..
            } => {
                assert_eq!(property_kind, PropertyKind::Overflow);
                assert!(message.contains("Counterexample"));
            }
            other => panic!("expected Violation, got {other:?}"),
        }
    }

    #[test]
    fn unproved_fixture_parses_as_unknown() {
        let r = parse_standard_json(UNKNOWN);
        match r {
            SmtCheckerResult::Unknown { reason, .. } => {
                assert!(reason.contains("could not be proved"));
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_is_error() {
        let r = parse_standard_json("not json");
        assert!(matches!(r, SmtCheckerResult::Error { .. }));
    }

    #[test]
    fn classify_property_dispatches_on_keywords() {
        assert_eq!(
            classify_property("CHC: Overflow happens"),
            PropertyKind::Overflow
        );
        assert_eq!(
            classify_property("CHC: Assertion violation happens here"),
            PropertyKind::Assertion
        );
        assert_eq!(
            classify_property("CHC: Division by zero"),
            PropertyKind::DivisionByZero
        );
    }

    #[test]
    fn count_parser_extracts_leading_number() {
        assert_eq!(
            parse_count_from_message("Info: CHC: 5 verification condition(s) proved safe!"),
            Some(5)
        );
        assert_eq!(parse_count_from_message("CHC: 12 things"), Some(12));
        assert_eq!(parse_count_from_message("not chc"), None);
    }
}
