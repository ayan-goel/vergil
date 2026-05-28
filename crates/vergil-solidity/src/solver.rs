//! Direct SMT-LIB dispatch to z3 / cvc5 / bitwuzla. Phase 4 Slice A2.
//!
//! Used by `vergil prove --solver <name>` to re-dispatch SMT-LIB queries
//! captured during a previous `vergil verify` run (via Phase 3's
//! `--dump-smt-queries`) without re-running Halmos. A property re-verifies
//! when the alternate solver also returns UNSAT.

use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Solver {
    Z3,
    Cvc5,
    Bitwuzla,
}

impl Solver {
    pub fn as_str(self) -> &'static str {
        match self {
            Solver::Z3 => "z3",
            Solver::Cvc5 => "cvc5",
            Solver::Bitwuzla => "bitwuzla",
        }
    }

    /// Parse a CLI-supplied solver name. Case-insensitive.
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "z3" => Some(Solver::Z3),
            "cvc5" => Some(Solver::Cvc5),
            "bitwuzla" => Some(Solver::Bitwuzla),
            _ => None,
        }
    }

    /// Return the "other" solver — used by `vergil prove` when no
    /// explicit `--solver` is passed: alternate from whatever
    /// originally produced the SMT-LIB query, so re-dispatch surfaces
    /// solver-specific bugs.
    pub fn alternate(self) -> Self {
        match self {
            Solver::Z3 => Solver::Cvc5,
            Solver::Cvc5 => Solver::Z3,
            // Bitwuzla alternates back to Z3 (no obvious symmetric partner).
            Solver::Bitwuzla => Solver::Z3,
        }
    }
}

/// Outcome of dispatching one SMT-LIB query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolverVerdict {
    /// Solver returned `unsat` — the property's negation is unsatisfiable,
    /// i.e. the property holds across the encoded domain.
    Unsat,
    /// Solver returned `sat` — there exists a model satisfying the negation,
    /// i.e. a counterexample to the property exists.
    Sat,
    /// Solver returned `unknown` — incomplete or timed out without a verdict.
    Unknown { reason: String },
}

#[derive(Debug, Error)]
pub enum SolverError {
    #[error("solver `{0}` not found on PATH")]
    NotInstalled(&'static str),
    #[error("solver `{name}` spawn failed: {source}")]
    Spawn {
        name: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("solver `{name}` exit failed: {message}")]
    NonZeroExit { name: &'static str, message: String },
    #[error("solver `{name}` wall-clock budget {ms}ms exceeded")]
    Timeout { name: &'static str, ms: u64 },
    #[error("solver `{name}` produced unparseable output:\n{raw}")]
    Unparseable { name: &'static str, raw: String },
}

/// Configuration for one SMT-LIB dispatch.
#[derive(Debug, Clone)]
pub struct SolverRun {
    pub solver: Solver,
    pub query: String,
    pub wall_clock_budget: Duration,
}

impl SolverRun {
    pub fn new(solver: Solver, query: impl Into<String>) -> Self {
        Self {
            solver,
            query: query.into(),
            wall_clock_budget: Duration::from_secs(60),
        }
    }

    pub fn with_wall_clock(mut self, budget: Duration) -> Self {
        self.wall_clock_budget = budget;
        self
    }
}

/// Dispatch one SMT-LIB query through the named solver. Reads stdout.
pub async fn dispatch(run: &SolverRun) -> Result<SolverVerdict, SolverError> {
    let binary = run.solver.as_str();
    let mut cmd = Command::new(binary);
    // Each solver takes a slightly different stdin flag:
    //   z3        -in            (reads SMT-LIB from stdin)
    //   cvc5      --lang=smt2    (auto-reads stdin when no positional arg)
    //   bitwuzla  --lang smt2    (same)
    match run.solver {
        Solver::Z3 => {
            cmd.arg("-in");
        }
        Solver::Cvc5 => {
            cmd.arg("--lang=smt2");
        }
        Solver::Bitwuzla => {
            cmd.arg("--lang").arg("smt2");
        }
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            SolverError::NotInstalled(binary)
        } else {
            SolverError::Spawn {
                name: binary,
                source,
            }
        }
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(run.query.as_bytes())
            .await
            .map_err(|source| SolverError::Spawn {
                name: binary,
                source,
            })?;
        drop(stdin);
    }

    let output = match timeout(run.wall_clock_budget, child.wait_with_output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(source)) => {
            return Err(SolverError::Spawn {
                name: binary,
                source,
            });
        }
        Err(_) => {
            return Err(SolverError::Timeout {
                name: binary,
                ms: run.wall_clock_budget.as_millis() as u64,
            });
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_verdict(binary, &stdout, &stderr)
}

/// Parse a solver's output for the first occurrence of `sat` / `unsat` /
/// `unknown`. Solvers can interleave info messages above; we look for the
/// first line that matches.
fn parse_verdict(
    name: &'static str,
    stdout: &str,
    stderr: &str,
) -> Result<SolverVerdict, SolverError> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        match trimmed {
            "unsat" => return Ok(SolverVerdict::Unsat),
            "sat" => return Ok(SolverVerdict::Sat),
            "unknown" => {
                return Ok(SolverVerdict::Unknown {
                    reason: extract_unknown_reason(stdout).unwrap_or_default(),
                });
            }
            _ => {}
        }
    }
    let combined = if stderr.is_empty() {
        stdout.to_string()
    } else {
        format!("stdout:\n{stdout}\nstderr:\n{stderr}")
    };
    Err(SolverError::Unparseable {
        name,
        raw: combined,
    })
}

fn extract_unknown_reason(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("(:reason-unknown ") {
            let end = rest.find(')').unwrap_or(rest.len());
            return Some(rest[..end].trim().trim_matches('"').to_string());
        }
    }
    None
}

/// Read a `.smt2` file from disk and dispatch it. Convenience helper for
/// `vergil prove`'s re-dispatch loop.
pub async fn dispatch_file(
    path: &std::path::Path,
    solver: Solver,
    budget: Duration,
) -> Result<SolverVerdict, SolverError> {
    let query = std::fs::read_to_string(path).map_err(|source| SolverError::Spawn {
        name: solver.as_str(),
        source,
    })?;
    dispatch(&SolverRun::new(solver, query).with_wall_clock(budget)).await
}

/// Hash a single SMT-LIB file's content as `sha256(bytes)`, lowercase hex.
/// Used to verify a persisted .smt2 matches the SHA the proof.json recorded.
pub fn file_sha256_hex(path: &std::path::Path) -> Result<String, std::io::Error> {
    use sha2::Digest;
    let bytes = std::fs::read(path)?;
    let mut h = sha2::Sha256::new();
    h.update(&bytes);
    let digest: [u8; 32] = h.finalize().into();
    let mut s = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for b in digest {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    Ok(s)
}

/// Where `vergil prove` looks for persisted SMT-LIB files.
/// Convention: `<project_root>/vergil-out/smt/<sha256>.smt2`.
pub fn smt_path_for(project_root: &std::path::Path, sha: &str) -> PathBuf {
    project_root
        .join("vergil-out")
        .join("smt")
        .join(format!("{sha}.smt2"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solver_from_name_is_case_insensitive() {
        assert_eq!(Solver::from_name("Z3"), Some(Solver::Z3));
        assert_eq!(Solver::from_name("cvc5"), Some(Solver::Cvc5));
        assert_eq!(Solver::from_name("BITWUZLA"), Some(Solver::Bitwuzla));
        assert_eq!(Solver::from_name("foo"), None);
    }

    #[test]
    fn alternate_swaps_z3_and_cvc5() {
        assert_eq!(Solver::Z3.alternate(), Solver::Cvc5);
        assert_eq!(Solver::Cvc5.alternate(), Solver::Z3);
        assert_eq!(Solver::Bitwuzla.alternate(), Solver::Z3);
    }

    #[test]
    fn parse_verdict_picks_first_keyword() {
        assert!(matches!(
            parse_verdict("z3", "unsat\n", "").unwrap(),
            SolverVerdict::Unsat
        ));
        assert!(matches!(
            parse_verdict("z3", "info\nsat\n", "").unwrap(),
            SolverVerdict::Sat
        ));
        let v = parse_verdict("z3", "unknown\n(:reason-unknown \"timeout\")", "").unwrap();
        match v {
            SolverVerdict::Unknown { reason } => assert_eq!(reason, "timeout"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn parse_verdict_unparseable_returns_error() {
        let err = parse_verdict("z3", "totally unrelated\n", "").unwrap_err();
        assert!(matches!(err, SolverError::Unparseable { .. }));
    }

    #[test]
    fn smt_path_uses_project_smt_dir() {
        let path = smt_path_for(std::path::Path::new("/tmp/proj"), "abc123");
        assert_eq!(path, PathBuf::from("/tmp/proj/vergil-out/smt/abc123.smt2"));
    }

    #[test]
    fn file_sha256_matches_handcomputed() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("q.smt2");
        std::fs::write(&p, b"(check-sat)").unwrap();
        let h = file_sha256_hex(&p).unwrap();
        // Hand-computed: sha256("(check-sat)") via the same routine
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(b"(check-sat)");
        let digest: [u8; 32] = hasher.finalize().into();
        let expected = digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(h, expected);
    }

    /// Live solver test — only runs if z3 is on PATH. Confirms the
    /// end-to-end dispatch path works against a real binary.
    #[tokio::test]
    async fn dispatch_returns_unsat_for_contradiction() {
        if which::which("z3").is_err() {
            eprintln!("skipping: z3 not on PATH");
            return;
        }
        let query = "(set-logic QF_BV)\n\
                     (declare-const x (_ BitVec 8))\n\
                     (assert (= x #x05))\n\
                     (assert (not (= x #x05)))\n\
                     (check-sat)\n";
        let run = SolverRun::new(Solver::Z3, query).with_wall_clock(Duration::from_secs(10));
        let v = dispatch(&run).await.unwrap();
        assert_eq!(v, SolverVerdict::Unsat);
    }
}
