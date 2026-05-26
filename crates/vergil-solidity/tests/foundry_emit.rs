//! End-to-end test of the Halmos → counterexample → forge test pipeline:
//!
//!   1. Run Halmos on the broken ERC-20 (expect Counterexample).
//!   2. Emit a Foundry test reproducing the cex.
//!   3. Run `forge test --match-test Cex_<property>` and assert it FAILS
//!      (forge exit code 1 + at least one failing test).
//!
//! Gated behind the `integration` feature.

#![cfg(feature = "integration")]

use std::path::PathBuf;
use std::time::Duration;

use tokio::process::Command;
use vergil_solidity::foundry::{emit_counterexample, PropertyContext};
use vergil_solidity::halmos::{run_simple, HalmosResult};

fn broken_project() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("examples");
    p.push("erc20-broken");
    p
}

#[tokio::test]
async fn counterexample_reproduces_via_forge() {
    let project = broken_project();
    let cex_test_path = project
        .join("test")
        .join("Cex_check_transferFrom_blocks_unauthorized.t.sol");
    let _ = std::fs::remove_file(&cex_test_path);

    // Step 1: run Halmos, get the counterexample.
    let result = run_simple(
        &project,
        "check_transferFrom_blocks_unauthorized",
        Duration::from_secs(60),
    )
    .await;

    let trace = match result {
        HalmosResult::Counterexample { trace, .. } => trace,
        other => panic!("expected Counterexample from broken ERC-20, got {other:?}"),
    };
    assert!(
        !trace.inputs.is_empty(),
        "expected at least one input in cex"
    );

    // Step 2: emit Foundry test.
    let ctx = PropertyContext {
        contract_name: "Properties",
        import_path: "./Properties.t.sol",
        params: &[("to", "address"), ("amount", "uint256")],
        constructor_args: &[],
    };
    let src = emit_counterexample(&trace, &ctx);
    std::fs::write(&cex_test_path, &src).expect("write cex test");

    // Step 3: ensure forge-std is available (the emitter imports from it).
    let forge_std_path = project.join("lib").join("forge-std");
    if !forge_std_path.is_dir() {
        let status = Command::new("forge")
            .arg("install")
            .arg("foundry-rs/forge-std")
            .current_dir(&project)
            .status()
            .await
            .expect("forge install");
        assert!(status.success(), "forge install forge-std failed");
    }

    // Step 4: run forge test, expect failure.
    let output = Command::new("forge")
        .arg("test")
        .arg("--match-test")
        .arg("test_Cex_check_transferFrom_blocks_unauthorized")
        .current_dir(&project)
        .output()
        .await
        .expect("run forge test");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("STDOUT:\n{stdout}\nSTDERR:\n{stderr}");

    assert!(
        !output.status.success(),
        "forge test should have failed but exited 0:\n{combined}"
    );
    assert!(
        stdout.contains("FAIL") || stdout.contains("Failing tests") || stdout.contains("1 failed"),
        "forge output didn't indicate a test failure:\n{combined}"
    );

    // Clean up the emitted file so the example dir stays pristine for repeat runs.
    let _ = std::fs::remove_file(&cex_test_path);
}
