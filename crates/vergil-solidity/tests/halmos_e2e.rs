//! End-to-end verification that the Halmos wrapper actually invokes Halmos
//! and that a hand-written property on the reference ERC-20 verifies.
//!
//! Gated behind the `integration` feature because it spawns external tools.

#![cfg(feature = "integration")]

use std::path::PathBuf;
use std::time::Duration;

use vergil_solidity::halmos::{run_simple, HalmosResult};

fn example_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.pop(); // repo root
    p.push("examples");
    p.push("erc20");
    p
}

#[tokio::test]
async fn erc20_transfer_preserves_total_supply_verifies() {
    let project = example_dir();
    assert!(
        project.join("foundry.toml").is_file(),
        "expected foundry project at {}",
        project.display()
    );

    let result = run_simple(
        &project,
        "check_transfer_preserves_total_supply",
        Duration::from_secs(60),
    )
    .await;

    match result {
        HalmosResult::Verified {
            property,
            paths,
            wall_clock_ms,
            smt_query_sha256: _, // dump flag not enabled by run_simple
        } => {
            assert!(property.starts_with("check_transfer_preserves_total_supply"));
            assert!(paths >= 1, "expected at least one path, got {paths}");
            assert!(
                wall_clock_ms < 60_000,
                "expected <60s wall clock, got {wall_clock_ms}ms"
            );
        }
        other => panic!("expected Verified, got {other:?}"),
    }
}

/// Confirms that when `dump_smt2` is enabled, `run()` either populates the
/// SMT hash from the dump dir OR cleanly returns `None` (verified paths
/// that didn't need solver invocation). Either is a valid outcome; the
/// negative shape is documented in [`HalmosResult::Verified::smt_query_sha256`].
#[tokio::test]
async fn dump_smt2_flag_does_not_break_verification() {
    use vergil_solidity::halmos::{run, HalmosRun};

    let project = example_dir();
    let dump_dir = std::env::temp_dir().join(format!("vergil-halmos-e2e-{}", std::process::id()));
    let cfg = HalmosRun::new(project, "check_transfer_preserves_total_supply")
        .with_wall_clock(std::time::Duration::from_secs(60))
        .with_dump_smt_directory(dump_dir);

    match run(&cfg).await {
        HalmosResult::Verified {
            smt_query_sha256, ..
        } => {
            // Hash may be populated (paths needed solver) or None
            // (Halmos resolved without solver invocation). Both are valid;
            // we only assert the shape: when present, it's 64 hex chars.
            if let Some(h) = smt_query_sha256 {
                assert_eq!(h.len(), 64, "SHA-256 hex should be 64 chars, got {h:?}");
                assert!(
                    h.chars().all(|c| c.is_ascii_hexdigit()),
                    "expected lowercase hex, got {h}"
                );
            }
        }
        other => panic!("expected Verified, got {other:?}"),
    }
}
