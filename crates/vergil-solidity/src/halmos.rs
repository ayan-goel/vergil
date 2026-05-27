//! Parser and (Slice 3) subprocess wrapper for Halmos symbolic-execution output.
//!
//! The parser converts Halmos stdout/stderr into a typed [`HalmosResult`]. The
//! fixtures under `tests/fixtures/halmos/` are real Halmos output captured during
//! Phase 1; synthetic variants (timeout, unknown) are modeled on the same format.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HalmosResult {
    /// Property proved over the symbolic domain Halmos explored.
    Verified {
        property: String,
        paths: u32,
        wall_clock_ms: u64,
        /// SHA-256 of the SMT-LIB queries Halmos dumped (when `--dump-smt-queries`
        /// is enabled via [`HalmosRun::dump_smt2`]). Halmos only writes `.smt2`
        /// files for paths the solver was actually invoked on; cleanly verified
        /// properties whose paths Halmos resolved without a solver call legitimately
        /// produce no dump, leaving this `None`. Phase 4's `vergil prove` uses the
        /// hash (when present) to skip re-running Halmos and re-dispatch the query
        /// directly to a solver.
        smt_query_sha256: Option<String>,
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
            smt_query_sha256: None,
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
    /// Whether to enable Halmos's `--dump-smt-queries` flag. When `true`,
    /// the runner allocates a temp directory (unless `dump_smt_directory`
    /// is set), invokes Halmos with `--dump-smt-queries --dump-smt-directory <path>`,
    /// then hashes the dumped `.smt2` files into [`HalmosResult::Verified::smt_query_sha256`].
    pub dump_smt2: bool,
    /// Explicit directory for SMT dumps. When `None` and `dump_smt2` is `true`,
    /// the runner creates a per-invocation temp directory under the system
    /// temp dir. Caller-owned cleanup; we don't delete anything.
    pub dump_smt_directory: Option<PathBuf>,
}

impl HalmosRun {
    pub fn new(project: impl Into<PathBuf>, check_fn: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            check_fn: check_fn.into(),
            wall_clock_budget: Duration::from_secs(120),
            solver_timeout_ms: 30_000,
            dump_smt2: false,
            dump_smt_directory: None,
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

    pub fn with_dump_smt2(mut self, enabled: bool) -> Self {
        self.dump_smt2 = enabled;
        self
    }

    pub fn with_dump_smt_directory(mut self, dir: PathBuf) -> Self {
        self.dump_smt_directory = Some(dir);
        self.dump_smt2 = true;
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

    let dump_dir = if cfg.dump_smt2 {
        resolve_dump_dir(cfg).ok()
    } else {
        None
    };

    let mut cmd = Command::new("halmos");
    cmd.arg("--function")
        .arg(&cfg.check_fn)
        .arg("--solver-timeout-assertion")
        .arg(cfg.solver_timeout_ms.to_string())
        .current_dir(&cfg.project)
        .env("HALMOS_ALLOW_DOWNLOAD", "1")
        .kill_on_drop(true);

    if let Some(ref dir) = dump_dir {
        cmd.arg("--dump-smt-queries")
            .arg("--dump-smt-directory")
            .arg(dir);
    }

    let result = timeout(cfg.wall_clock_budget, cmd.output()).await;
    match result {
        Ok(Ok(output)) => {
            let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
            combined.push_str(&String::from_utf8_lossy(&output.stderr));
            let mut parsed = parse(&combined);
            if let (Some(dir), HalmosResult::Verified { .. }) = (&dump_dir, &parsed) {
                if let Some(hash) = hash_smt_dump(dir).ok().flatten() {
                    if let HalmosResult::Verified {
                        smt_query_sha256, ..
                    } = &mut parsed
                    {
                        *smt_query_sha256 = Some(hash);
                    }
                }
            }
            parsed
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

fn resolve_dump_dir(cfg: &HalmosRun) -> Result<PathBuf, std::io::Error> {
    if let Some(p) = &cfg.dump_smt_directory {
        std::fs::create_dir_all(p)?;
        return Ok(p.clone());
    }
    let base = std::env::temp_dir().join(format!(
        "vergil-halmos-smt-{}-{}",
        sanitize_for_path(&cfg.check_fn),
        std::process::id()
    ));
    std::fs::create_dir_all(&base)?;
    Ok(base)
}

fn sanitize_for_path(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Walk `dir` recursively, collect every `.smt2` file (skipping `.smt2.out`
/// solver-response files), sort by relative path, and return a SHA-256
/// digest over the concatenated bytes (with each file's relative path
/// prefixed for stability across reordering).
///
/// Returns `Ok(None)` when the directory exists but contains no `.smt2`
/// files (verified properties whose paths Halmos resolved without invoking
/// the solver — see [`HalmosResult::Verified::smt_query_sha256`]).
pub fn hash_smt_dump(dir: &Path) -> Result<Option<String>, std::io::Error> {
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    collect_smt_files(dir, dir, &mut entries)?;
    if entries.is_empty() {
        return Ok(None);
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (rel, path) in &entries {
        hasher.update(rel.as_bytes());
        hasher.update(b"\0");
        let bytes = std::fs::read(path)?;
        hasher.update(&bytes);
        hasher.update(b"\0");
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Ok(Some(hex_lower(&digest)))
}

fn collect_smt_files(
    root: &Path,
    cur: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(cur)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_smt_files(root, &path, out)?;
            continue;
        }
        if !path.extension().map(|e| e == "smt2").unwrap_or(false) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        out.push((rel, path));
    }
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
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
    const CEX_ALLOWANCE: &str =
        include_str!("../tests/fixtures/halmos/counterexample-allowance.txt");

    #[test]
    fn verified_parses() {
        let r = parse(VERIFIED);
        match r {
            HalmosResult::Verified {
                property,
                paths,
                wall_clock_ms,
                smt_query_sha256,
            } => {
                assert!(
                    property.starts_with("check_commutative"),
                    "property = {property}"
                );
                assert_eq!(paths, 1);
                assert!(wall_clock_ms > 0);
                // The parser alone never fills the SMT hash; that happens in
                // `run()` post-process from the dump directory.
                assert!(smt_query_sha256.is_none());
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
    fn counterexample_allowance_parses() {
        let r = parse(CEX_ALLOWANCE);
        match r {
            HalmosResult::Counterexample { trace, .. } => {
                assert!(trace
                    .property
                    .starts_with("check_transferFrom_blocks_unauthorized"));
                assert_eq!(trace.inputs.len(), 2);
                let names: Vec<&str> = trace.inputs.iter().map(|i| i.name.as_str()).collect();
                assert!(names.contains(&"amount"), "names = {names:?}");
                assert!(names.contains(&"to"), "names = {names:?}");
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

    #[test]
    fn hash_smt_dump_returns_none_for_empty_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let hash = hash_smt_dump(tmp.path()).unwrap();
        assert!(hash.is_none(), "expected None for empty dir, got {hash:?}");
    }

    #[test]
    fn hash_smt_dump_is_stable_and_matches_handcomputed_sha256() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("check_x");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("0.smt2"), b"(assert true)").unwrap();
        std::fs::write(sub.join("1.smt2"), b"(check-sat)").unwrap();
        // .smt2.out files are solver responses, not queries, and must be skipped.
        std::fs::write(sub.join("0.smt2.out"), b"sat").unwrap();

        let h1 = hash_smt_dump(tmp.path()).unwrap().unwrap();
        let h2 = hash_smt_dump(tmp.path()).unwrap().unwrap();
        assert_eq!(h1, h2, "hash should be stable across calls");
        assert_eq!(h1.len(), 64, "hash should be 64 hex chars");

        // Hand-computed equivalent: rebuild the input the function digested.
        let mut hasher = Sha256::new();
        for (rel, content) in [
            ("check_x/0.smt2", &b"(assert true)"[..]),
            ("check_x/1.smt2", &b"(check-sat)"[..]),
        ] {
            hasher.update(rel.as_bytes());
            hasher.update(b"\0");
            hasher.update(content);
            hasher.update(b"\0");
        }
        let expected = hex_lower(&<[u8; 32]>::from(hasher.finalize()));
        assert_eq!(h1, expected, "hash should match hand-computed SHA-256");
    }

    #[test]
    fn hash_smt_dump_is_order_independent() {
        let tmp_a = tempfile::tempdir().unwrap();
        let tmp_b = tempfile::tempdir().unwrap();
        for tmp in [&tmp_a, &tmp_b] {
            let sub = tmp.path().join("check_y");
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join("9.smt2"), b"nine").unwrap();
            std::fs::write(sub.join("2.smt2"), b"two").unwrap();
            std::fs::write(sub.join("11.smt2"), b"eleven").unwrap();
        }
        // The two dirs are identical in content; their hashes must agree even
        // though `read_dir` may yield entries in different orders.
        let h1 = hash_smt_dump(tmp_a.path()).unwrap().unwrap();
        let h2 = hash_smt_dump(tmp_b.path()).unwrap().unwrap();
        assert_eq!(h1, h2);
    }
}
