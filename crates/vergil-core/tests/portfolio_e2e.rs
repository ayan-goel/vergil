//! E2E: dispatch Halmos + SMTChecker on the reference ERC-20 concurrently
//! and assert the winning verdict is Verified.

#![cfg(feature = "integration")]

use std::path::PathBuf;
use std::time::Duration;

use vergil_core::portfolio::{dispatch, PortfolioConfig, Verdict};

fn erc20_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("examples");
    p.push("erc20");
    p
}

#[tokio::test]
async fn portfolio_verifies_safemath_via_winner() {
    let project = erc20_dir();
    let smt_source = project.join("src").join("SafeMath.sol");

    let cfg = PortfolioConfig {
        project: project.clone(),
        property: "check_transfer_preserves_total_supply".to_string(),
        smtchecker_source: smt_source,
        budget: Duration::from_secs(120),
        capture_smt_queries: false,
    };

    let result = dispatch(cfg).await;
    match result.verdict {
        Verdict::Verified {
            backend,
            wall_clock_ms,
            ..
        } => {
            assert!(
                wall_clock_ms < 120_000,
                "wall clock too long: {wall_clock_ms}ms"
            );
            eprintln!(
                "portfolio winner: {backend:?} (property = {}) in {wall_clock_ms}ms",
                result.property
            );
        }
        other => panic!("expected Verified, got {other:?}"),
    }
}
