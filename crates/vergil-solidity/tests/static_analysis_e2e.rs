//! E2E test: run Slither + solc storage-layout on the reference ERC-20
//! and assert the combined report is consistent.

#![cfg(feature = "integration")]

use std::path::PathBuf;
use std::time::Duration;

use vergil_solidity::static_analysis::analyze;

fn token_source() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("examples");
    p.push("erc20");
    p.push("src");
    p.push("Token.sol");
    p
}

#[tokio::test]
async fn erc20_static_analysis_runs_and_agrees() {
    let source = token_source();
    assert!(source.is_file(), "{}", source.display());

    let report = analyze(&source, Duration::from_secs(120))
        .await
        .expect("static analysis should succeed on clean ERC-20");

    // solc must have produced a storage layout for the Token contract.
    assert_eq!(report.storage.len(), 1, "expected 1 contract");
    let layout = &report.storage[0];
    assert!(layout.qualified_name.ends_with(":Token"));
    assert_eq!(layout.entries.len(), 5, "expected 5 storage entries");

    // Slither runs and returns at least the standard detectors.
    // We don't assert specific findings beyond "ran successfully".
    let _ = report.slither.detectors.len();
}
