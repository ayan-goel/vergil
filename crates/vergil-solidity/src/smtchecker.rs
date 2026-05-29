//! Wrapper around solc's built-in SMTChecker (CHC mode).
//!
//! We invoke `solc --standard-json` to get structured JSON output. The model
//! checker emits diagnostic objects under `errors[]`; we classify them into
//! Verified / Violation / Unknown / Error.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::process::Command;
use tokio::time::timeout;

/// Prefix solc uses for the printed CHC query when
/// `settings.modelChecker.printQuery = true`.
const QUERY_PREFIX: &str = "CHC: Requested query:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmtCheckerResult {
    /// All targets the model checker considered were proved safe.
    Verified {
        proved_safe_count: u32,
        wall_clock_ms: u64,
        /// SHA-256 of the CHC query SMTChecker dumped when
        /// `--model-checker-print-query smtlib2` was enabled (Slice 3).
        /// `None` for runs without query capture or when solc emitted no
        /// SMT-LIB body in its output. Phase 4 uses this for re-dispatch.
        smt_query_sha256: Option<String>,
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
    let mut chc_diag_count = 0;
    for diag in &output.errors {
        if !is_chc_message(&diag.message) {
            continue;
        }
        chc_diag_count += 1;
        if diag.message.contains("proved safe") {
            if let Some(n) = parse_count_from_message(&diag.message) {
                aggregated = aggregated.max(n);
            }
        } else if diag.message.contains("check is safe") {
            individual += 1;
        }
    }

    // If solc emitted zero CHC messages, the model checker didn't actually
    // engage (e.g., no solver linked into solc and no external solver found).
    // Reporting Verified here would be a lie — return Unknown so callers know.
    if chc_diag_count == 0 {
        return SmtCheckerResult::Unknown {
            reason: "solc emitted no CHC diagnostics (model checker did not engage)".to_string(),
            wall_clock_ms: 0,
        };
    }

    let proved_safe = aggregated.max(individual);
    SmtCheckerResult::Verified {
        proved_safe_count: proved_safe,
        wall_clock_ms: 0,
        smt_query_sha256: None,
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
    /// Whether to set `settings.modelChecker.printQuery = true`, which causes
    /// solc to emit the raw SMTLIB2 query as an `info` diagnostic. The parser
    /// extracts those query bodies, concatenates them in emission order,
    /// SHA-256-digests, and stores in [`SmtCheckerResult::Verified::smt_query_sha256`].
    pub print_query: bool,
}

impl SmtCheckerRun {
    pub fn new(project: impl Into<PathBuf>, source: impl Into<PathBuf>) -> Self {
        Self {
            project: project.into(),
            source: source.into(),
            wall_clock_budget: Duration::from_secs(120),
            solver_timeout_ms: 30_000,
            print_query: false,
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

    pub fn with_print_query(mut self, enabled: bool) -> Self {
        self.print_query = enabled;
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

    let mut model_checker = serde_json::json!({
        "engine": "chc",
        "targets": ["assert", "overflow", "underflow", "divByZero", "outOfBounds"],
        "timeout": cfg.solver_timeout_ms,
        "showProvedSafe": true,
        // Use both built-in (if solc was linked with z3) and external z3 (via
        // smtlib2). On some Linux solc binaries z3 isn't linked in; falling
        // back to smtlib2 finds the z3 binary on PATH.
        "solvers": ["z3", "smtlib2"],
    });
    // `printQuery` (Slice 3) is only recognized by newer solc — solc 0.8.20
    // rejects the *key itself* with `Unknown key "printQuery"` even when the
    // value is false. So only emit it when query capture is requested; the
    // default path then stays compatible with any solc the host provides
    // (CI pins 0.8.20). Callers that need query capture run on a newer solc.
    if cfg.print_query {
        model_checker["printQuery"] = serde_json::Value::Bool(true);
    }

    let input = serde_json::json!({
        "language": "Solidity",
        "sources": { source_key.clone(): { "content": content } },
        "settings": {
            "modelChecker": model_checker,
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
    if cfg.print_query {
        if let SmtCheckerResult::Verified {
            smt_query_sha256, ..
        } = &mut parsed
        {
            *smt_query_sha256 = extract_query_hash(&stdout);
        }
    }
    parsed
}

/// Extract every CHC query body from a solc standard-json output, concatenate
/// in emission order, and SHA-256 the result. Returns `None` when no such
/// diagnostic exists (printQuery was not honored, or solc didn't emit any).
fn extract_query_hash(stdout: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(stdout).ok()?;
    let errors = parsed.get("errors")?.as_array()?;
    let mut hasher = Sha256::new();
    let mut found = false;
    for e in errors {
        let msg = e.get("message").and_then(|m| m.as_str())?;
        let Some(body) = msg.strip_prefix(QUERY_PREFIX) else {
            continue;
        };
        found = true;
        // Trim leading whitespace/newlines but keep trailing structure intact.
        let body = body.trim_start();
        hasher.update(body.as_bytes());
        hasher.update(b"\0");
    }
    if !found {
        return None;
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Some(hex_lower_smt(&digest))
}

fn hex_lower_smt(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
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

    #[test]
    fn extract_query_hash_returns_none_when_no_query_diagnostic() {
        let stdout = serde_json::json!({
            "errors": [
                {"severity": "warning", "message": "CHC: 1 verification condition(s) proved safe!"}
            ]
        })
        .to_string();
        assert!(extract_query_hash(&stdout).is_none());
    }

    #[test]
    fn extract_query_hash_returns_some_when_printquery_emitted_a_diagnostic() {
        let stdout = serde_json::json!({
            "errors": [
                {"severity": "info", "message": "CHC: Requested query:\n(set-logic HORN)\n(check-sat)\n"},
                {"severity": "warning", "message": "CHC: 1 verification condition(s) proved safe!"}
            ]
        })
        .to_string();
        let h = extract_query_hash(&stdout).expect("expected Some hash");
        assert_eq!(h.len(), 64, "SHA-256 hex should be 64 chars");

        // Hand-computed expectation: the body the function digests is the
        // trimmed query body + a single null byte separator.
        let mut hasher = Sha256::new();
        hasher.update(b"(set-logic HORN)\n(check-sat)\n");
        hasher.update(b"\0");
        let expected = hex_lower_smt(&<[u8; 32]>::from(hasher.finalize()));
        assert_eq!(h, expected);
    }

    #[test]
    fn extract_query_hash_is_stable_and_concatenates_multiple_queries() {
        let stdout = serde_json::json!({
            "errors": [
                {"severity": "info", "message": "CHC: Requested query:\n(query-1)\n"},
                {"severity": "info", "message": "CHC: Requested query:\n(query-2)\n"},
            ]
        })
        .to_string();
        let h = extract_query_hash(&stdout).unwrap();
        assert_eq!(h.len(), 64);
        // Stability: same input → same hash.
        let h2 = extract_query_hash(&stdout).unwrap();
        assert_eq!(h, h2);
    }
}
