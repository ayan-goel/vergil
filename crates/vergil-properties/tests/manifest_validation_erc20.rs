//! Integration test (--features integration): validate the
//! erc20-sum-of-balances template against examples/erc20-broken via real
//! solc + slither subprocesses. Confirms the validator surfaces a
//! StorageSlotMismatch when the manifest's `_balances` / `_totalSupply`
//! names don't appear in the actual contract layout (the example uses
//! `balanceOf` / `totalSupply` as the public-getter names, not `_balances`).

#![cfg(feature = "integration")]

use std::path::PathBuf;
use std::time::Duration;

use vergil_properties::{validate, Catalog, ManifestError};
use vergil_solidity::slither::{run_simple as slither_run, SlitherResult};
use vergil_solidity::storage::{run_simple as storage_run, StorageResult};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[tokio::test]
async fn validate_reference_template_against_erc20_broken() {
    let cat = Catalog::load(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates"))
        .expect("catalog load");
    let tmpl = cat.get("erc20-sum-of-balances").expect("ref template");

    let src = repo_root().join("examples/erc20-broken/src/TokenBroken.sol");
    let budget = Duration::from_secs(60);

    let storage = match storage_run(&src, budget).await {
        StorageResult::Ok(l) => l,
        StorageResult::Error(e) => panic!("solc storage layout failed: {e}"),
    };
    let slither = match slither_run(&src, budget).await {
        SlitherResult::Ok(r) => r,
        SlitherResult::Error(e) => panic!("slither run failed: {e}"),
    };

    // The reference template declares _balances and _totalSupply; the
    // example contract uses balanceOf / totalSupply as state names. We
    // expect StorageSlotMissing on _balances — proving the validator
    // catches manifest/contract drift before the solver runs.
    match validate(&tmpl.manifest, &storage, &slither) {
        Err(ManifestError::StorageSlotMissing { slot, .. }) => {
            assert_eq!(slot, "_balances");
        }
        Err(other) => panic!("unexpected error: {other}"),
        Ok(_) => panic!("validation should fail: manifest names don't match contract"),
    }
}
