//! `vergil prove <artifact>` — re-verify a proof artifact against the
//! current filesystem state.

use std::path::PathBuf;

use vergil_proof::{verify_artifact, ProveError};

pub fn run(artifact: PathBuf) -> Result<(), u8> {
    match verify_artifact(&artifact, None) {
        Ok(report) => {
            println!(
                "vergil prove — {}:\n  source files re-hashed: {}\n  verified properties recorded: {}\n  re-dispatch attempted: {} (Phase 3 carry-over)",
                report.artifact_path.display(),
                report.source_files_rehashed,
                report.verified_properties,
                report.re_dispatch_attempted
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("vergil prove failed: {e}");
            match e {
                ProveError::SourceShaMismatch { .. } | ProveError::Schema(_) => Err(1),
                _ => Err(3),
            }
        }
    }
}
