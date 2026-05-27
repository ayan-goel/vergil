//! `vergil prove` re-verification.
//!
//! Given a path to a `proof.json`, this:
//!   1. Deserializes the artifact and runs structural validation.
//!   2. Re-hashes every `source_files` entry; mismatch is a hard failure.
//!   3. (Phase 3) re-dispatches each `verified_properties.smt_query_sha256`
//!      through the named solver and asserts UNSAT. Phase 2 ships the
//!      schema + hash-recheck path; SMT re-dispatch lands when SMT-LIB
//!      capture is wired into the Halmos/SMTChecker wrappers.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::schema::{sha256_hex, ProofArtifact};

#[derive(Debug, Error)]
pub enum ProveError {
    #[error("read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("schema invalid: {0}")]
    Schema(String),
    #[error(
        "source file `{path}` SHA256 mismatch (artifact says {expected}, current is {actual})"
    )]
    SourceShaMismatch {
        path: String,
        expected: String,
        actual: String,
    },
}

#[derive(Debug, Clone)]
pub struct ProveReport {
    pub artifact_path: PathBuf,
    pub source_files_rehashed: usize,
    pub verified_properties: usize,
    pub re_dispatch_attempted: usize,
}

/// Re-verify a proof artifact against the current filesystem.
///
/// `project_root_override` lets the caller relocate the run (e.g. unpack
/// an artifact in a fresh clone where the original absolute path doesn't
/// exist). When `None`, the artifact's recorded `run.project_root` is used.
pub fn verify_artifact(
    artifact_path: &Path,
    project_root_override: Option<&Path>,
) -> Result<ProveReport, ProveError> {
    let bytes = std::fs::read(artifact_path).map_err(|e| ProveError::Io {
        path: artifact_path.to_path_buf(),
        source: e,
    })?;
    let artifact: ProofArtifact =
        serde_json::from_slice(&bytes).map_err(|e| ProveError::Parse {
            path: artifact_path.to_path_buf(),
            source: e,
        })?;
    artifact.validate().map_err(ProveError::Schema)?;

    let root = project_root_override
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(&artifact.run.project_root));

    let mut rehashed = 0usize;
    for f in &artifact.source_files {
        let path = root.join(&f.path);
        let body = std::fs::read(&path).map_err(|e| ProveError::Io {
            path: path.clone(),
            source: e,
        })?;
        let actual = sha256_hex(&body);
        if actual != f.sha256 {
            return Err(ProveError::SourceShaMismatch {
                path: f.path.clone(),
                expected: f.sha256.clone(),
                actual,
            });
        }
        rehashed += 1;
    }

    Ok(ProveReport {
        artifact_path: artifact_path.to_path_buf(),
        source_files_rehashed: rehashed,
        verified_properties: artifact.verified_properties.len(),
        // Phase 2: SMT re-dispatch is documented as Phase 3 carry-over per
        // the verify.rs module docstring; the artifact already names the
        // backend + solver + spec SHA, so an external re-verifier can do
        // it manually until Slice 14's Halmos --dump-smt2 integration lands.
        re_dispatch_attempted: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;
    use tempfile::tempdir;

    fn artifact_with_source(path: &str, body: &[u8]) -> (ProofArtifact, Vec<u8>) {
        let sha = sha256_hex(body);
        let a = ProofArtifact {
            vergil_version: "0.0.1".into(),
            schema_version: 1,
            run: RunMeta {
                run_id: "r".into(),
                intent: "i".into(),
                project_root: ".".into(),
                started_at: "2026-05-26T00:00:00Z".into(),
            },
            toolchain: ToolchainVersions {
                solc: "0.8.20".into(),
                halmos: "0.3.3".into(),
                slither: "0.11.0".into(),
                z3: "4.15.4".into(),
                cvc5: "1.3.0".into(),
                gambit: None,
            },
            source_files: vec![SourceFile {
                path: path.into(),
                sha256: sha,
            }],
            verified_properties: Vec::new(),
            counterexamples: Vec::new(),
            quality_metrics: QualityMetrics {
                mutation_coverage_min: None,
                critique_pass_rate: 0.0,
                mutation_testing_enabled: false,
            },
            cost: Cost {
                tokens_in: 0,
                tokens_out: 0,
                usd_estimate: 0.0,
                wall_clock_ms: 0,
            },
        };
        (a, body.to_vec())
    }

    #[test]
    fn rehashes_and_returns_report() {
        let tmp = tempdir().unwrap();
        let body = b"clean source body";
        let (artifact, body_vec) = artifact_with_source("src/T.sol", body);
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/T.sol"), &body_vec).unwrap();
        let proof_path = tmp.path().join("proof.json");
        std::fs::write(&proof_path, serde_json::to_string(&artifact).unwrap()).unwrap();

        let report = verify_artifact(&proof_path, Some(tmp.path())).unwrap();
        assert_eq!(report.source_files_rehashed, 1);
        assert_eq!(report.verified_properties, 0);
    }

    #[test]
    fn sha_mismatch_is_hard_failure() {
        let tmp = tempdir().unwrap();
        let body = b"original";
        let (artifact, _) = artifact_with_source("src/T.sol", body);
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/T.sol"), b"TAMPERED").unwrap();
        let proof_path = tmp.path().join("proof.json");
        std::fs::write(&proof_path, serde_json::to_string(&artifact).unwrap()).unwrap();

        let err = verify_artifact(&proof_path, Some(tmp.path())).unwrap_err();
        assert!(matches!(err, ProveError::SourceShaMismatch { .. }), "{err}");
    }

    #[test]
    fn missing_source_returns_io_error() {
        let tmp = tempdir().unwrap();
        let (artifact, _) = artifact_with_source("src/Ghost.sol", b"any");
        let proof_path = tmp.path().join("proof.json");
        std::fs::write(&proof_path, serde_json::to_string(&artifact).unwrap()).unwrap();
        let err = verify_artifact(&proof_path, Some(tmp.path())).unwrap_err();
        assert!(matches!(err, ProveError::Io { .. }), "{err}");
    }
}
