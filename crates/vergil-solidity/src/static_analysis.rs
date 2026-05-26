//! Combines Slither (structural) and solc (storage) into a single analysis
//! pass. solc is authoritative for storage layout; any Slither-derived view
//! of storage must agree with it. Phase 1 implements a minimal cross-check
//! (presence of storage-relevant Slither warnings). Phase 2 will extend to
//! a slot-by-slot comparison once Slither's variable-order printer is wired.

use std::path::Path;
use std::time::Duration;

use thiserror::Error;

use crate::slither::{self, SlitherReport, SlitherResult};
use crate::storage::{self, StorageLayout, StorageResult};

#[derive(Debug, Error)]
pub enum StaticAnalysisError {
    #[error("slither failed: {0}")]
    SlitherFailed(String),
    #[error("solc storage layout failed: {0}")]
    StorageFailed(String),
    #[error("slither and solc disagree on storage: {0}")]
    SlitherStorageMismatch(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticAnalysisReport {
    pub slither: SlitherReport,
    pub storage: Vec<StorageLayout>,
}

/// Run both Slither and solc storage-layout on `source` concurrently and
/// fold the results into a single report. solc is authoritative — any
/// disagreement with Slither's storage view is fatal.
pub async fn analyze(
    source: &Path,
    budget: Duration,
) -> Result<StaticAnalysisReport, StaticAnalysisError> {
    let (sl_res, st_res) = tokio::join!(
        slither::run_simple(source, budget),
        storage::run_simple(source, budget),
    );

    let slither_report = match sl_res {
        SlitherResult::Ok(r) => r,
        SlitherResult::Error(e) => return Err(StaticAnalysisError::SlitherFailed(e)),
    };
    let storage_layouts = match st_res {
        StorageResult::Ok(l) => l,
        StorageResult::Error(e) => return Err(StaticAnalysisError::StorageFailed(e)),
    };

    cross_check(&slither_report, &storage_layouts)?;

    Ok(StaticAnalysisReport {
        slither: slither_report,
        storage: storage_layouts,
    })
}

/// Phase-1 cross-check: if Slither flagged any storage-related findings
/// (e.g. uninitialized-state, storage-array, reentrancy on storage),
/// we surface them. A real slot-by-slot comparison lands in Phase 2.
fn cross_check(
    slither: &SlitherReport,
    storage: &[StorageLayout],
) -> Result<(), StaticAnalysisError> {
    // If solc reports no contracts, that's a hard failure: no layout means we
    // can't reason about anything else.
    if storage.is_empty() {
        return Err(StaticAnalysisError::SlitherStorageMismatch(
            "solc returned no storage layouts".to_string(),
        ));
    }

    // Surface high-impact storage-related Slither findings as a mismatch.
    // (Heuristic until Phase 2 wires Slither's variable-order printer.)
    for det in &slither.detectors {
        if det.impact == "High" && is_storage_relevant(&det.check) {
            return Err(StaticAnalysisError::SlitherStorageMismatch(format!(
                "slither {} ({}) on storage layout",
                det.check, det.impact
            )));
        }
    }

    Ok(())
}

fn is_storage_relevant(check: &str) -> bool {
    matches!(
        check,
        "uninitialized-state"
            | "storage-array"
            | "uninitialized-storage"
            | "storage-collision"
            | "reentrancy-eth"
            | "reentrancy-no-eth"
            | "incorrect-shift"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slither::Detector;
    use std::collections::HashMap;

    fn report(detectors: Vec<Detector>) -> SlitherReport {
        SlitherReport { detectors }
    }

    fn layout(name: &str) -> StorageLayout {
        StorageLayout {
            qualified_name: name.to_string(),
            entries: Vec::new(),
            types: HashMap::new(),
        }
    }

    #[test]
    fn empty_storage_is_mismatch() {
        let sl = report(Vec::new());
        let err = cross_check(&sl, &[]).unwrap_err();
        match err {
            StaticAnalysisError::SlitherStorageMismatch(msg) => {
                assert!(msg.contains("no storage layouts"));
            }
            other => panic!("expected mismatch, got {other:?}"),
        }
    }

    #[test]
    fn high_impact_storage_finding_blocks() {
        let sl = report(vec![Detector {
            check: "uninitialized-state".to_string(),
            impact: "High".to_string(),
            confidence: "High".to_string(),
            description: String::new(),
        }]);
        let err = cross_check(&sl, &[layout("X")]).unwrap_err();
        assert!(matches!(
            err,
            StaticAnalysisError::SlitherStorageMismatch(_)
        ));
    }

    #[test]
    fn benign_findings_pass() {
        let sl = report(vec![Detector {
            check: "constable-states".to_string(),
            impact: "Optimization".to_string(),
            confidence: "High".to_string(),
            description: String::new(),
        }]);
        cross_check(&sl, &[layout("X")]).expect("benign findings should not block");
    }
}
