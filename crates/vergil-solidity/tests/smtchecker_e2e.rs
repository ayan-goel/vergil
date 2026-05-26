//! End-to-end test: invoke solc's SMTChecker on the reference ERC-20
//! and assert it returns Verified (no overflow / assertion violations).

#![cfg(feature = "integration")]

use std::path::PathBuf;
use std::time::Duration;

use vergil_solidity::smtchecker::{run_simple, SmtCheckerResult};

fn example_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("examples");
    p.push("erc20");
    p
}

#[tokio::test]
async fn smtchecker_verifies_safemath_overflow_freedom() {
    let project = example_dir();
    let source = project.join("src").join("SafeMath.sol");
    assert!(source.is_file(), "expected source at {}", source.display());

    let result = run_simple(&project, &source, Duration::from_secs(120)).await;
    match result {
        SmtCheckerResult::Verified {
            proved_safe_count,
            wall_clock_ms,
        } => {
            assert!(
                proved_safe_count >= 1,
                "expected ≥1 proved-safe target, got {proved_safe_count}"
            );
            assert!(
                wall_clock_ms < 120_000,
                "wall clock too long: {wall_clock_ms}ms"
            );
        }
        SmtCheckerResult::Unknown { reason, .. } => {
            panic!("SMTChecker returned Unknown — increase budget or simplify: {reason}");
        }
        other => panic!("expected Verified, got {other:?}"),
    }
}
