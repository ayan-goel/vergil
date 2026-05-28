//! `vergil prove <artifact>` — re-verify a proof artifact against the
//! current filesystem state.
//!
//! Phase 4 Slice A2 adds SMT-LIB re-dispatch: for every
//! `verified_properties[].smt_query_sha256`, the CLI looks up the
//! persisted `.smt2` file under `<project_root>/vergil-out/smt/<sha>.smt2`
//! and re-dispatches it through an alternate solver (default cvc5). The
//! property "re-verifies" when the new solver also returns UNSAT.

use std::path::{Path, PathBuf};
use std::time::Duration;

use vergil_proof::schema::ProofArtifact;
use vergil_proof::{verify_artifact, ProveError};
use vergil_solidity::solver::{
    dispatch_file, file_sha256_hex, smt_path_for, Solver, SolverError, SolverVerdict,
};

/// Per-property re-dispatch outcome surfaced in the CLI.
#[derive(Debug)]
enum RedispatchOutcome {
    /// SMT file existed, alternate solver also returned UNSAT.
    Reverified,
    /// SMT file existed but alternate solver disagreed (sat or unknown).
    Mismatch { detail: String },
    /// proof.json carried a `smt_query_sha256` but the persisted .smt2
    /// file isn't on disk — original capture probably ran without
    /// `smt_persist_dir`. Surface as a warning, not a hard fail.
    Missing,
    /// proof.json's `smt_query_sha256` was null (the backend didn't dump,
    /// e.g. trivially-resolved paths).
    NoHashRecorded,
    /// Spawning the solver failed (binary not installed, IO error).
    SolverError { detail: String },
    /// On-disk .smt2 file's content SHA didn't match what proof.json
    /// recorded — the persisted artifact has been tampered with.
    HashMismatch { recorded: String, actual: String },
}

/// Convenience entrypoint used by tests; production callers go through
/// [`run_with_solver`] via the CLI's `--solver` flag.
#[allow(dead_code)]
pub fn run(artifact: PathBuf) -> Result<(), u8> {
    run_with_solver(artifact, None)
}

pub fn run_with_solver(artifact: PathBuf, solver_override: Option<String>) -> Result<(), u8> {
    // 1. Source-SHA recheck (Phase 2 behavior, unchanged).
    let base_report = match verify_artifact(&artifact, None) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("vergil prove failed: {e}");
            return Err(match e {
                ProveError::SourceShaMismatch { .. } | ProveError::Schema(_) => 1,
                _ => 3,
            });
        }
    };

    // 2. Re-read the artifact for the full properties list.
    let artifact_bytes = match std::fs::read(&artifact) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("vergil prove: re-read artifact: {e}");
            return Err(3);
        }
    };
    let parsed: ProofArtifact = match serde_json::from_slice(&artifact_bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("vergil prove: re-parse artifact: {e}");
            return Err(3);
        }
    };

    let project_root = Path::new(&parsed.run.project_root).to_path_buf();

    // 3. Decide which solver to dispatch against.
    let chosen_solver = match resolve_solver_choice(solver_override.as_deref()) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("vergil prove: {msg}");
            return Err(3);
        }
    };

    // 4. Re-dispatch each verified property (if it has a hash).
    let outcomes = redispatch_all(&parsed, &project_root, chosen_solver);

    // 5. Report aggregated counts.
    let mut reverified = 0;
    let mut mismatched = 0;
    let mut missing = 0;
    let mut errored = 0;
    let mut no_hash = 0;
    let mut tampered = 0;
    for (name, outcome) in &outcomes {
        match outcome {
            RedispatchOutcome::Reverified => reverified += 1,
            RedispatchOutcome::Mismatch { detail } => {
                mismatched += 1;
                eprintln!("  ✗ {name}: alternate solver disagrees — {detail}");
            }
            RedispatchOutcome::Missing => missing += 1,
            RedispatchOutcome::SolverError { detail } => {
                errored += 1;
                eprintln!("  ! {name}: solver dispatch failed — {detail}");
            }
            RedispatchOutcome::NoHashRecorded => no_hash += 1,
            RedispatchOutcome::HashMismatch { recorded, actual } => {
                tampered += 1;
                eprintln!(
                    "  ✗ {name}: persisted .smt2 SHA mismatch — recorded={recorded}, actual={actual}"
                );
            }
        }
    }

    println!(
        "vergil prove — {}:\n  source files re-hashed: {}\n  verified properties recorded: {}\n  re-dispatched via {}: {} reverified, {} mismatch, {} missing, {} no-hash, {} tampered, {} solver-error",
        base_report.artifact_path.display(),
        base_report.source_files_rehashed,
        base_report.verified_properties,
        chosen_solver.as_str(),
        reverified,
        mismatched,
        missing,
        no_hash,
        tampered,
        errored,
    );

    // Exit non-zero if any mismatch or tamper happened (these are soundness
    // signals). Missing / no-hash / solver-error are warnings.
    if mismatched > 0 || tampered > 0 {
        return Err(1);
    }
    Ok(())
}

/// Decide which solver to dispatch against. CLI override wins; otherwise
/// default to cvc5 (Halmos's primary is z3, so cvc5 surfaces solver-
/// specific anomalies on re-dispatch).
fn resolve_solver_choice(override_name: Option<&str>) -> Result<Solver, String> {
    if let Some(name) = override_name {
        return Solver::from_name(name)
            .ok_or_else(|| format!("unknown solver `{name}` (expected: z3, cvc5, bitwuzla)"));
    }
    Ok(Solver::Cvc5)
}

fn redispatch_all(
    artifact: &ProofArtifact,
    project_root: &Path,
    solver: Solver,
) -> Vec<(String, RedispatchOutcome)> {
    let mut out = Vec::new();
    for prop in &artifact.verified_properties {
        let outcome = match &prop.smt_query_sha256 {
            None => RedispatchOutcome::NoHashRecorded,
            Some(sha) => {
                let smt_path = smt_path_for(project_root, sha);
                if !smt_path.is_file() {
                    RedispatchOutcome::Missing
                } else {
                    // Verify the persisted file's SHA actually matches.
                    match file_sha256_hex(&smt_path) {
                        Ok(actual) if &actual == sha => dispatch_one(&smt_path, solver),
                        Ok(actual) => RedispatchOutcome::HashMismatch {
                            recorded: sha.clone(),
                            actual,
                        },
                        Err(e) => RedispatchOutcome::SolverError {
                            detail: format!("hash {}: {e}", smt_path.display()),
                        },
                    }
                }
            }
        };
        out.push((prop.name.clone(), outcome));
    }
    out
}

fn dispatch_one(path: &Path, solver: Solver) -> RedispatchOutcome {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            return RedispatchOutcome::SolverError {
                detail: format!("build runtime: {e}"),
            };
        }
    };
    let path_buf = path.to_path_buf();
    let result =
        rt.block_on(async move { dispatch_file(&path_buf, solver, Duration::from_secs(60)).await });
    match result {
        Ok(SolverVerdict::Unsat) => RedispatchOutcome::Reverified,
        Ok(SolverVerdict::Sat) => RedispatchOutcome::Mismatch {
            detail: "alternate solver returned `sat` (counterexample exists)".to_string(),
        },
        Ok(SolverVerdict::Unknown { reason }) => RedispatchOutcome::Mismatch {
            detail: format!("alternate solver returned `unknown`: {reason}"),
        },
        Err(SolverError::NotInstalled(name)) => RedispatchOutcome::SolverError {
            detail: format!("{name} not on PATH — install or pass --solver <name>"),
        },
        Err(e) => RedispatchOutcome::SolverError {
            detail: format!("{e}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_solver_default_is_cvc5() {
        let s = resolve_solver_choice(None).unwrap();
        assert_eq!(s, Solver::Cvc5);
    }

    #[test]
    fn resolve_solver_accepts_override() {
        assert_eq!(resolve_solver_choice(Some("z3")).unwrap(), Solver::Z3);
        assert_eq!(resolve_solver_choice(Some("Z3")).unwrap(), Solver::Z3);
        assert_eq!(
            resolve_solver_choice(Some("bitwuzla")).unwrap(),
            Solver::Bitwuzla
        );
    }

    #[test]
    fn resolve_solver_rejects_unknown_name() {
        let err = resolve_solver_choice(Some("foo")).unwrap_err();
        assert!(err.contains("unknown solver"));
    }
}
