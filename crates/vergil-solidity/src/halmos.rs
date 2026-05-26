//! Parser and (Slice 3) subprocess wrapper for Halmos symbolic-execution output.
//!
//! The parser converts Halmos stdout/stderr into a typed [`HalmosResult`]. The
//! fixtures under `tests/fixtures/halmos/` are real Halmos output captured during
//! Phase 1; synthetic variants (timeout, unknown) are modeled on the same format.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HalmosResult {
    /// Property proved over the symbolic domain Halmos explored.
    Verified {
        property: String,
        paths: u32,
        wall_clock_ms: u64,
    },
    /// Concrete inputs that falsify the property.
    Counterexample { trace: Trace, wall_clock_ms: u64 },
    /// Solver returned `unknown` or hit a non-timeout error during solving.
    Unknown {
        property: String,
        reason: String,
        wall_clock_ms: u64,
    },
    /// Solver timed out before reaching a verdict.
    Timeout {
        property: String,
        wall_clock_ms: u64,
    },
    /// Halmos itself failed (build error, syntax error, invalid invocation).
    Error { stage: ErrorStage, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorStage {
    /// `forge build` failed (e.g. Solidity syntax error).
    Build,
    /// Halmos crashed or returned a non-recoverable error at runtime.
    Runtime,
}

impl fmt::Display for ErrorStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorStage::Build => f.write_str("build"),
            ErrorStage::Runtime => f.write_str("runtime"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Trace {
    pub property: String,
    pub inputs: Vec<NamedValue>,
    pub storage_writes: Vec<StorageWrite>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedValue {
    /// Logical input name reconstructed from the Halmos symbol (e.g. `a` from `p_a_uint256_...`).
    pub name: String,
    /// Concrete hex value as Halmos reported it (e.g. `0xff..f3`).
    pub hex_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageWrite {
    pub slot: String,
    pub value: String,
}

/// Parse Halmos combined stdout+stderr output into a typed result.
///
/// Halmos prints ANSI color codes by default; the parser strips them before
/// matching. Output that doesn't match any known shape becomes an
/// [`ErrorStage::Runtime`] error preserving the original text for diagnostics.
pub fn parse(raw: &str) -> HalmosResult {
    let text = strip_ansi(raw);

    if let Some(err) = parse_build_error(&text) {
        return err;
    }

    let property = extract_property(&text).unwrap_or_else(|| "<unknown>".to_string());
    let wall_clock_ms = extract_total_time_ms(&text).unwrap_or(0);

    if let Some(pass) = find_status_line(&text, "PASS") {
        let paths = extract_paths(&pass).unwrap_or(0);
        return HalmosResult::Verified {
            property,
            paths,
            wall_clock_ms,
        };
    }

    if find_status_line(&text, "TIMEOUT").is_some() {
        return HalmosResult::Timeout {
            property,
            wall_clock_ms,
        };
    }

    if find_status_line(&text, "UNKNOWN").is_some() {
        let reason =
            extract_warning(&text).unwrap_or_else(|| "solver returned unknown".to_string());
        return HalmosResult::Unknown {
            property,
            reason,
            wall_clock_ms,
        };
    }

    if find_status_line(&text, "FAIL").is_some() {
        let inputs = extract_counterexample_inputs(&text);
        let trace = Trace {
            property: property.clone(),
            inputs,
            storage_writes: Vec::new(),
        };
        return HalmosResult::Counterexample {
            trace,
            wall_clock_ms,
        };
    }

    if find_status_line(&text, "ERROR").is_some() {
        return HalmosResult::Error {
            stage: ErrorStage::Runtime,
            message: extract_error_message(&text).unwrap_or_else(|| text.clone()),
        };
    }

    HalmosResult::Error {
        stage: ErrorStage::Runtime,
        message: format!("unrecognized halmos output:\n{}", text.trim()),
    }
}

/// Configuration for a Halmos subprocess invocation.
#[derive(Debug, Clone)]
pub struct HalmosRun {
    /// Foundry project directory (the dir containing `foundry.toml`).
    pub project: PathBuf,
    /// `check_*` function name to verify.
    pub check_fn: String,
    /// Wall-clock budget for the entire halmos invocation.
    pub wall_clock_budget: Duration,
    /// Solver budget per assertion, milliseconds. Forwarded as
    /// `--solver-timeout-assertion <ms>`.
    pub solver_timeout_ms: u64,
}

impl HalmosRun {
    pub fn new(project: impl Into<PathBuf>, check_fn: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            check_fn: check_fn.into(),
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

/// Spawn Halmos as a subprocess in the given Foundry project, parse its output.
///
/// On wall-clock budget exhaustion the child process is killed and a
/// [`HalmosResult::Timeout`] is returned. Solver-side timeouts (individual
/// assertions) surface from Halmos itself in the parsed output.
pub async fn run(cfg: &HalmosRun) -> HalmosResult {
    if !cfg.project.join("foundry.toml").is_file() {
        return HalmosResult::Error {
            stage: ErrorStage::Runtime,
            message: format!("no foundry.toml in project dir: {}", cfg.project.display()),
        };
    }

    let mut cmd = Command::new("halmos");
    cmd.arg("--function")
        .arg(&cfg.check_fn)
        .arg("--solver-timeout-assertion")
        .arg(cfg.solver_timeout_ms.to_string())
        .current_dir(&cfg.project)
        .env("HALMOS_ALLOW_DOWNLOAD", "1")
        .kill_on_drop(true);

    let result = timeout(cfg.wall_clock_budget, cmd.output()).await;
    match result {
        Ok(Ok(output)) => {
            let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
            combined.push_str(&String::from_utf8_lossy(&output.stderr));
            parse(&combined)
        }
        Ok(Err(io_err)) => HalmosResult::Error {
            stage: ErrorStage::Runtime,
            message: format!("failed to spawn halmos: {io_err}"),
        },
        Err(_elapsed) => HalmosResult::Timeout {
            property: cfg.check_fn.clone(),
            wall_clock_ms: cfg.wall_clock_budget.as_millis() as u64,
        },
    }
}

/// Convenience wrapper preserving the function signature called out in
/// `tasks/plan.md`: `async fn run(project, check_fn, budget)`.
pub async fn run_simple(project: &Path, check_fn: &str, budget: Duration) -> HalmosResult {
    let cfg = HalmosRun::new(project.to_path_buf(), check_fn.to_string()).with_wall_clock(budget);
    run(&cfg).await
}

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        if chars.peek() != Some(&'[') {
            continue;
        }
        chars.next();
        for next in chars.by_ref() {
            if next.is_ascii_alphabetic() {
                break;
            }
        }
    }
    out
}

fn parse_build_error(text: &str) -> Option<HalmosResult> {
    if text.contains("Build failed:") || text.contains("Compiler run failed:") {
        let message = text
            .lines()
            .filter(|line| {
                line.contains("Error")
                    || line.contains("error:")
                    || line.contains("Compiler run failed")
                    || line.contains("Build failed")
                    || line.trim_start().starts_with("|")
                    || line.trim_start().starts_with("--> ")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let message = if message.is_empty() {
            text.to_string()
        } else {
            message
        };
        return Some(HalmosResult::Error {
            stage: ErrorStage::Build,
            message,
        });
    }
    None
}

fn find_status_line(text: &str, tag: &str) -> Option<String> {
    let needle = format!("[{tag}]");
    text.lines()
        .find(|l| l.contains(&needle))
        .map(str::to_string)
}

fn extract_property(text: &str) -> Option<String> {
    for line in text.lines() {
        for tag in &["[PASS]", "[FAIL]", "[TIMEOUT]", "[UNKNOWN]", "[ERROR]"] {
            if let Some(idx) = line.find(tag) {
                let after = &line[idx + tag.len()..];
                let trimmed = after.trim_start();
                let end = trimmed
                    .find('(')
                    .map(|i| {
                        let before_paren = &trimmed[..i];
                        if let Some(sig_start) = trimmed[i..].find(')') {
                            i + sig_start + 1
                        } else {
                            before_paren.len()
                        }
                    })
                    .unwrap_or(trimmed.len());
                return Some(trimmed[..end].trim().to_string());
            }
        }
    }
    None
}

fn extract_paths(status_line: &str) -> Option<u32> {
    let start = status_line.find("paths:")? + "paths:".len();
    let rest = &status_line[start..];
    let comma = rest.find(',').unwrap_or(rest.len());
    rest[..comma].trim().parse().ok()
}

fn extract_total_time_ms(text: &str) -> Option<u64> {
    let line = text
        .lines()
        .rev()
        .find(|l| l.contains("Symbolic test result"))?;
    let idx = line.rfind("time:")?;
    let secs = line[idx + "time:".len()..]
        .trim()
        .trim_end_matches('s')
        .parse::<f64>()
        .ok()?;
    Some((secs * 1000.0) as u64)
}

fn extract_warning(text: &str) -> Option<String> {
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("WARNING"))?;
    Some(line.trim_start_matches("WARNING").trim().to_string())
}

fn extract_error_message(text: &str) -> Option<String> {
    let line = text.lines().find(|l| l.trim_start().starts_with("ERROR"))?;
    Some(line.trim_start_matches("ERROR").trim().to_string())
}

fn extract_counterexample_inputs(text: &str) -> Vec<NamedValue> {
    let mut inputs = Vec::new();
    let mut in_cex = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Counterexample") {
            in_cex = true;
            continue;
        }
        if !in_cex {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('[') || trimmed.starts_with("Symbolic") {
            break;
        }
        if let Some(input) = parse_cex_line(trimmed) {
            inputs.push(input);
        }
    }
    inputs
}

fn parse_cex_line(line: &str) -> Option<NamedValue> {
    let (lhs, rhs) = line.split_once('=')?;
    let raw_name = lhs.trim();
    let hex = rhs.trim();
    if !hex.starts_with("0x") {
        return None;
    }
    let name = recover_logical_name(raw_name);
    Some(NamedValue {
        name,
        hex_value: hex.to_string(),
    })
}

/// Halmos emits symbol names like `p_a_uint256_<hash>_00`. Strip the `p_` prefix,
/// drop the trailing `_<hash>_NN`, and remove the type suffix — leaving `a`.
fn recover_logical_name(raw: &str) -> String {
    let stripped = raw.strip_prefix("p_").unwrap_or(raw);
    let parts: Vec<&str> = stripped.split('_').collect();
    if parts.len() <= 2 {
        return parts[0].to_string();
    }
    // The logical name ends just before the first part that looks like a Solidity type.
    let type_keywords = [
        "uint256", "uint128", "uint64", "uint32", "uint16", "uint8", "int256", "int128", "int64",
        "int32", "int16", "int8", "address", "bool", "bytes32", "bytes",
    ];
    let mut name_end = parts.len();
    for (i, part) in parts.iter().enumerate() {
        if type_keywords.contains(part) {
            name_end = i;
            break;
        }
    }
    if name_end == 0 {
        parts[0].to_string()
    } else {
        parts[..name_end].join("_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERIFIED: &str = include_str!("../tests/fixtures/halmos/verified.txt");
    const CEX_OVERFLOW: &str = include_str!("../tests/fixtures/halmos/counterexample-overflow.txt");
    const TIMEOUT: &str = include_str!("../tests/fixtures/halmos/timeout.txt");
    const UNKNOWN: &str = include_str!("../tests/fixtures/halmos/unknown.txt");
    const ERROR_SYNTAX: &str = include_str!("../tests/fixtures/halmos/error-syntax.txt");

    #[test]
    fn verified_parses() {
        let r = parse(VERIFIED);
        match r {
            HalmosResult::Verified {
                property,
                paths,
                wall_clock_ms,
            } => {
                assert!(
                    property.starts_with("check_commutative"),
                    "property = {property}"
                );
                assert_eq!(paths, 1);
                assert!(wall_clock_ms > 0);
            }
            other => panic!("expected Verified, got {other:?}"),
        }
    }

    #[test]
    fn counterexample_parses_with_inputs() {
        let r = parse(CEX_OVERFLOW);
        match r {
            HalmosResult::Counterexample { trace, .. } => {
                assert!(trace.property.starts_with("check_overflow_bug"));
                assert_eq!(trace.inputs.len(), 2);
                let names: Vec<&str> = trace.inputs.iter().map(|i| i.name.as_str()).collect();
                assert!(names.contains(&"a"), "names = {names:?}");
                assert!(names.contains(&"b"), "names = {names:?}");
                for input in &trace.inputs {
                    assert!(input.hex_value.starts_with("0x"));
                }
            }
            other => panic!("expected Counterexample, got {other:?}"),
        }
    }

    #[test]
    fn timeout_parses() {
        let r = parse(TIMEOUT);
        assert!(matches!(r, HalmosResult::Timeout { .. }), "got {r:?}");
    }

    #[test]
    fn unknown_parses_with_reason() {
        let r = parse(UNKNOWN);
        match r {
            HalmosResult::Unknown { reason, .. } => {
                assert!(reason.contains("unknown") || reason.contains("Z3"));
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn error_syntax_parses_as_build_error() {
        let r = parse(ERROR_SYNTAX);
        match r {
            HalmosResult::Error { stage, message } => {
                assert_eq!(stage, ErrorStage::Build);
                assert!(message.contains("Compiler run failed") || message.contains("Error"));
            }
            other => panic!("expected build Error, got {other:?}"),
        }
    }

    #[test]
    fn strip_ansi_removes_color_codes() {
        let input = "\x1b[32m[PASS]\x1b[0m foo";
        assert_eq!(strip_ansi(input), "[PASS] foo");
    }

    #[test]
    fn strip_ansi_preserves_unstyled_text() {
        assert_eq!(strip_ansi("plain text\n"), "plain text\n");
    }

    #[test]
    fn unrecognized_output_becomes_runtime_error() {
        let r = parse("totally unrelated output\n");
        assert!(matches!(
            r,
            HalmosResult::Error {
                stage: ErrorStage::Runtime,
                ..
            }
        ));
    }

    #[test]
    fn recover_logical_name_strips_halmos_decoration() {
        assert_eq!(recover_logical_name("p_a_uint256_9ce4ac8_00"), "a");
        assert_eq!(recover_logical_name("p_owner_address_abc123_00"), "owner");
        assert_eq!(recover_logical_name("p_x_bool_ff_00"), "x");
    }
}
