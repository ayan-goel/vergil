//! Integration test (--features integration): drive real Gambit against a
//! tiny Solidity file. Confirms the binary is reachable, generates a
//! non-empty mutant set, and the result JSON parses into Mutant records.

#![cfg(feature = "integration")]

use std::time::Duration;
use tempfile::tempdir;

use vergil_mutation::{MutationError, Mutator};

#[tokio::test]
async fn gambit_generates_mutants_on_tiny_contract() {
    let tmp = tempdir().unwrap();
    let source = tmp.path().join("Tiny.sol");
    std::fs::write(
        &source,
        "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract T { uint256 public x; function add(uint256 a) public { x += a; } }\n",
    )
    .unwrap();
    let outdir = tmp.path().join("gambit_out");

    let mut m = Mutator::default();
    m.budget = Duration::from_secs(30);

    let mutants = match m.generate(&source, &outdir).await {
        Ok(v) => v,
        Err(MutationError::GambitMissing) => {
            eprintln!("gambit not installed; skipping integration test");
            return;
        }
        Err(other) => panic!("gambit failed: {other}"),
    };
    assert!(
        !mutants.is_empty(),
        "expected ≥1 mutant on a non-trivial contract"
    );
    assert!(mutants.iter().all(|m| !m.id.is_empty()));
    assert!(mutants.iter().all(|m| m.name.ends_with("Tiny.sol")));
}
