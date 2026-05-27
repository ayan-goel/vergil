//! Gambit subprocess wrapper. Generates mutants and parses the
//! `gambit_results.json` it emits into typed [`Mutant`] records.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Error)]
pub enum MutationError {
    #[error(
        "gambit not on PATH; install with `cargo install --git https://github.com/Certora/gambit`"
    )]
    GambitMissing,
    #[error("gambit spawn: {0}")]
    Spawn(String),
    #[error("gambit exited {status}: {stderr}")]
    NonZeroExit { status: i32, stderr: String },
    #[error("gambit wall-clock budget exceeded")]
    Timeout,
    #[error("gambit results not found at {0}")]
    MissingResults(PathBuf),
    #[error("parse gambit results: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mutant {
    pub id: String,
    pub description: String,
    /// Diff text Gambit emits in the result JSON.
    #[serde(default)]
    pub diff: String,
    /// Relative path under `outdir` to the mutated file (e.g.
    /// `mutants/1/Test.sol`). Resolve against the outdir to read it.
    pub name: String,
    /// The path Gambit thinks the source was relative to. Mutants are
    /// applied by overwriting `original` with the mutant's file content.
    pub original: String,
    pub sourceroot: String,
}

/// Parse the JSON array Gambit writes to `<outdir>/gambit_results.json`.
pub fn parse_results(raw: &str) -> Result<Vec<Mutant>, MutationError> {
    serde_json::from_str::<Vec<Mutant>>(raw).map_err(|e| MutationError::Parse(format!("{e}")))
}

#[derive(Debug, Clone)]
pub struct Mutator {
    pub binary: PathBuf,
    pub budget: Duration,
}

impl Default for Mutator {
    fn default() -> Self {
        Self {
            binary: PathBuf::from("gambit"),
            budget: Duration::from_secs(120),
        }
    }
}

impl Mutator {
    /// Run `gambit mutate --filename <source> --outdir <outdir>` and parse
    /// the resulting JSON. Returns [`MutationError::GambitMissing`] if the
    /// binary cannot be spawned (PATH).
    pub async fn generate(
        &self,
        source: &Path,
        outdir: &Path,
    ) -> Result<Vec<Mutant>, MutationError> {
        if !source.exists() {
            return Err(MutationError::Spawn(format!(
                "source not found: {}",
                source.display()
            )));
        }
        // Gambit overwrites the outdir by default; --no_overwrite would
        // print a warning and exit if the outdir exists. To stay forwards-
        // compatible we clear-and-recreate ourselves so the default keeps
        // working regardless of Gambit's future default.
        let _ = std::fs::remove_dir_all(outdir);
        std::fs::create_dir_all(outdir)
            .map_err(|e| MutationError::Spawn(format!("create outdir: {e}")))?;

        let sourceroot = source
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let mut cmd = Command::new(&self.binary);
        cmd.arg("mutate")
            .arg("--filename")
            .arg(source)
            .arg("--outdir")
            .arg(outdir)
            .arg("--sourceroot")
            .arg(&sourceroot)
            .kill_on_drop(true);

        let result = timeout(self.budget, cmd.output()).await;
        let output = match result {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => {
                if let std::io::ErrorKind::NotFound = e.kind() {
                    return Err(MutationError::GambitMissing);
                }
                return Err(MutationError::Spawn(format!("{e}")));
            }
            Err(_) => return Err(MutationError::Timeout),
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(MutationError::NonZeroExit {
                status: output.status.code().unwrap_or(-1),
                stderr,
            });
        }
        let results_path = outdir.join("gambit_results.json");
        if !results_path.exists() {
            return Err(MutationError::MissingResults(results_path));
        }
        let raw = std::fs::read_to_string(&results_path)
            .map_err(|e| MutationError::Spawn(format!("read results: {e}")))?;
        parse_results(&raw)
    }

    pub fn is_available(&self) -> bool {
        // Best-effort: try `gambit --help`. Returns false on spawn error.
        std::process::Command::new(&self.binary)
            .arg("--help")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMOKE_RESULTS: &str = r#"[
        {
            "id": "1",
            "description": "DeleteExpressionMutation",
            "diff": "--- original\n+++ mutant\n@@ x += a ==> assert(true)",
            "name": "mutants/1/Test.sol",
            "original": "Test.sol",
            "sourceroot": "/tmp/smoke"
        },
        {
            "id": "2",
            "description": "AssignmentMutation",
            "diff": "--- original\n+++ mutant\n@@ a ==> 0",
            "name": "mutants/2/Test.sol",
            "original": "Test.sol",
            "sourceroot": "/tmp/smoke"
        }
    ]"#;

    #[test]
    fn parse_results_round_trips() {
        let v = parse_results(SMOKE_RESULTS).expect("parse");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].id, "1");
        assert_eq!(v[0].description, "DeleteExpressionMutation");
        assert!(v[0].name.ends_with("Test.sol"));
    }

    #[test]
    fn parse_results_garbage_is_parse_error() {
        let err = parse_results("not json").unwrap_err();
        assert!(matches!(err, MutationError::Parse(_)), "{err}");
    }

    #[test]
    fn missing_source_is_spawn_error() {
        let m = Mutator::default();
        let fake = PathBuf::from("/tmp/this-path-does-not-exist-vergil-test.sol");
        let outdir = tempfile::tempdir().unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(m.generate(&fake, outdir.path()))
            .expect_err("missing source must error");
        assert!(matches!(err, MutationError::Spawn(_)), "{err}");
    }
}
