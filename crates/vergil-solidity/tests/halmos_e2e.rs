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
