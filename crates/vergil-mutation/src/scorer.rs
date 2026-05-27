//! Mutation scoring.
//!
//! For each mutant: overwrite the original Solidity source with the mutant's
//! version, invoke a caller-supplied [`MutationRunner`] (which knows how to
//! run Halmos against the current spec), restore the original. The spec
//! "kills" a mutant when the runner reports a counterexample / violation.
//! Coverage = killed / total.
//!
//! The runner indirection keeps this crate independent of `vergil-solidity`:
//! Slice 13's CEGIS orchestration constructs the runner closure with the
//! current Halmos config; the scorer just drives it.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use thiserror::Error;

use crate::gambit::Mutant;

#[derive(Debug, Error)]
pub enum ScoreError {
    #[error("file io {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("mutant {id} runner: {message}")]
    Runner { id: String, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationVerdict {
    /// Spec produced a counterexample / violation when this mutant was applied.
    Killed,
    /// Spec still verified — the mutant slipped through.
    Survived,
    /// Runner could not produce a verdict (build error, timeout, etc).
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MutationScore {
    pub total: usize,
    pub killed: usize,
    pub survived: usize,
    pub inconclusive: usize,
}

impl MutationScore {
    pub fn coverage(&self) -> f64 {
        let evaluated = self.killed + self.survived;
        if evaluated == 0 {
            0.0
        } else {
            self.killed as f64 / evaluated as f64
        }
    }
}

/// Trait the caller supplies. `run` is given the path to the contract
/// source (now overwritten with the mutant) and must return a verdict.
#[async_trait]
pub trait MutationRunner: Send + Sync {
    async fn run(&self, source: &Path) -> Result<MutationVerdict, String>;
}

pub struct MutationScorer<'a> {
    runner: &'a dyn MutationRunner,
    /// Path to the Gambit outdir whose `mutants/<id>/<file>.sol` we'll read.
    outdir: PathBuf,
}

impl<'a> MutationScorer<'a> {
    pub fn new(runner: &'a dyn MutationRunner, outdir: impl Into<PathBuf>) -> Self {
        Self {
            runner,
            outdir: outdir.into(),
        }
    }

    /// Score `spec` against every mutant by applying-and-running each.
    /// `original_source` is the path that gets overwritten; the original
    /// content is saved and restored.
    pub async fn score(
        &self,
        original_source: &Path,
        mutants: &[Mutant],
    ) -> Result<MutationScore, ScoreError> {
        let saved = std::fs::read(original_source).map_err(|e| ScoreError::Io {
            path: original_source.to_path_buf(),
            source: e,
        })?;

        let mut score = MutationScore {
            total: mutants.len(),
            killed: 0,
            survived: 0,
            inconclusive: 0,
        };

        let restore = |body: &[u8]| -> Result<(), ScoreError> {
            std::fs::write(original_source, body).map_err(|e| ScoreError::Io {
                path: original_source.to_path_buf(),
                source: e,
            })
        };

        for m in mutants {
            // Read mutant body from gambit's outdir.
            let mutant_path = self.outdir.join(&m.name);
            let body = match std::fs::read(&mutant_path) {
                Ok(b) => b,
                Err(_) => {
                    score.inconclusive += 1;
                    continue;
                }
            };
            // Overwrite source with mutant. Always restore before returning.
            if let Err(e) = std::fs::write(original_source, &body) {
                let _ = restore(&saved);
                return Err(ScoreError::Io {
                    path: original_source.to_path_buf(),
                    source: e,
                });
            }
            let v = self.runner.run(original_source).await;
            // Restore eagerly so a panic later doesn't leave the user's tree dirty.
            restore(&saved)?;
            match v {
                Ok(MutationVerdict::Killed) => score.killed += 1,
                Ok(MutationVerdict::Survived) => score.survived += 1,
                Ok(MutationVerdict::Inconclusive) => score.inconclusive += 1,
                Err(msg) => {
                    tracing::warn!("mutant {} runner error: {msg}", m.id);
                    score.inconclusive += 1;
                }
            }
        }

        Ok(score)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::tempdir;

    struct ScriptedRunner {
        verdicts: Mutex<Vec<MutationVerdict>>,
    }

    #[async_trait]
    impl MutationRunner for ScriptedRunner {
        async fn run(&self, _source: &Path) -> Result<MutationVerdict, String> {
            let mut v = self.verdicts.lock().unwrap();
            Ok(v.remove(0))
        }
    }

    #[tokio::test]
    async fn coverage_is_killed_over_evaluated() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("Original.sol");
        std::fs::write(&src, b"original body").unwrap();
        let outdir = tmp.path().join("out");
        std::fs::create_dir_all(outdir.join("mutants/1")).unwrap();
        std::fs::create_dir_all(outdir.join("mutants/2")).unwrap();
        std::fs::create_dir_all(outdir.join("mutants/3")).unwrap();
        std::fs::write(outdir.join("mutants/1/Original.sol"), b"mutant 1").unwrap();
        std::fs::write(outdir.join("mutants/2/Original.sol"), b"mutant 2").unwrap();
        std::fs::write(outdir.join("mutants/3/Original.sol"), b"mutant 3").unwrap();
        let mutants = vec![
            Mutant {
                id: "1".into(),
                description: "x".into(),
                diff: String::new(),
                name: "mutants/1/Original.sol".into(),
                original: "Original.sol".into(),
                sourceroot: "/tmp".into(),
            },
            Mutant {
                id: "2".into(),
                description: "x".into(),
                diff: String::new(),
                name: "mutants/2/Original.sol".into(),
                original: "Original.sol".into(),
                sourceroot: "/tmp".into(),
            },
            Mutant {
                id: "3".into(),
                description: "x".into(),
                diff: String::new(),
                name: "mutants/3/Original.sol".into(),
                original: "Original.sol".into(),
                sourceroot: "/tmp".into(),
            },
        ];
        let runner = ScriptedRunner {
            verdicts: Mutex::new(vec![
                MutationVerdict::Killed,
                MutationVerdict::Survived,
                MutationVerdict::Killed,
            ]),
        };
        let scorer = MutationScorer::new(&runner, outdir);
        let s = scorer.score(&src, &mutants).await.unwrap();
        assert_eq!(s.total, 3);
        assert_eq!(s.killed, 2);
        assert_eq!(s.survived, 1);
        assert_eq!(s.inconclusive, 0);
        assert!((s.coverage() - 2.0 / 3.0).abs() < 1e-6);
        // Source must be restored to the original.
        assert_eq!(std::fs::read(&src).unwrap(), b"original body");
    }

    #[tokio::test]
    async fn missing_mutant_file_counts_inconclusive() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("Original.sol");
        std::fs::write(&src, b"original").unwrap();
        let outdir = tmp.path().join("out");
        std::fs::create_dir_all(&outdir).unwrap();
        let mutants = vec![Mutant {
            id: "ghost".into(),
            description: "x".into(),
            diff: String::new(),
            name: "mutants/ghost/Original.sol".into(),
            original: "Original.sol".into(),
            sourceroot: "/tmp".into(),
        }];
        let runner = ScriptedRunner {
            verdicts: Mutex::new(vec![]),
        };
        let scorer = MutationScorer::new(&runner, outdir);
        let s = scorer.score(&src, &mutants).await.unwrap();
        assert_eq!(s.inconclusive, 1);
        assert_eq!(s.killed, 0);
    }

    #[test]
    fn coverage_zero_when_no_evaluation() {
        let s = MutationScore {
            total: 5,
            killed: 0,
            survived: 0,
            inconclusive: 5,
        };
        assert_eq!(s.coverage(), 0.0);
    }
}
