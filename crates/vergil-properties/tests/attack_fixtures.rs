//! Per-attack-template fixture tests (SPEC §9.1).
//!
//! For each attack template in the catalog, this test:
//!   1. Loads the template via [`AttackCatalog::load`].
//!   2. Renders the Halmos encoding with a [`RenderContext`] matching the
//!      template's fixtures.
//!   3. Drops the rendered encoding + the fixture into a temp Foundry
//!      project.
//!   4. Runs Halmos via the existing [`vergil_solidity::halmos`] wrapper.
//!   5. Asserts the vulnerable fixture → Counterexample; clean fixture →
//!      Verified.
//!
//! Gated on `--features integration` because each case spawns
//! `forge build` + `halmos` and takes tens of seconds.

#![cfg(feature = "integration")]

use std::path::{Path, PathBuf};
use std::time::Duration;

use vergil_properties::{render, AttackCatalog, AttackTemplate, RenderContext};
use vergil_solidity::halmos::{run_simple, HalmosResult};

fn templates_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join("attacks")
}

fn write(p: &Path, body: &str) {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, body).unwrap();
}

const FOUNDRY_TOML: &str = r#"[profile.default]
src = "src"
test = "test"
out = "out"
libs = ["lib"]
solc = "0.8.20"
optimizer = true
optimizer_runs = 200
"#;

/// Build a temp Foundry project containing `target_src` at `src/Target.sol`
/// and `check_src` at `test/AttackCheck.t.sol`. Returns the project root.
fn prepare_project(label: &str, target_src: &str, check_src: &str) -> tempfile::TempDir {
    let tmp = tempfile::Builder::new()
        .prefix(&format!("vergil-attack-{label}-"))
        .tempdir()
        .expect("tempdir");
    let root = tmp.path();
    write(&root.join("foundry.toml"), FOUNDRY_TOML);
    write(&root.join("src/Target.sol"), target_src);
    write(&root.join("test/AttackCheck.t.sol"), check_src);
    tmp
}

/// Standard render context for templates whose fixtures expose a
/// `Target { setProtected(uint256), protectedValue() }` surface.
fn access_modifier_render_ctx(attack_id_ident: &str) -> RenderContext {
    RenderContext::from_pairs([
        ("contract_name", "Target"),
        ("contract_path", "src/Target.sol"),
        ("setter", "setProtected"),
        ("getter", "protectedValue"),
        ("attack_id_ident", attack_id_ident),
    ])
}

fn ident_for(id: &str) -> String {
    id.replace('-', "_")
}

fn load_template(id: &str) -> AttackTemplate {
    let cat = AttackCatalog::load(templates_dir()).expect("attack catalog loads");
    cat.get(id)
        .cloned()
        .unwrap_or_else(|| panic!("template {id} not in catalog"))
}

const HALMOS_BUDGET: Duration = Duration::from_secs(120);

// ─── access-missing-modifier-state-change ────────────────────────────────────

#[tokio::test]
async fn access_missing_modifier_vulnerable_produces_counterexample() {
    let t = load_template("access-missing-modifier-state-change");
    let ctx = access_modifier_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("access-vuln", &t.vulnerable_source, &check);

    let result = run_simple(
        project.path(),
        "check_unauthorized_caller_cannot_mutate",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Counterexample { .. } => {
            // Expected: vulnerable Target lacks the modifier, so the
            // attacker contract's call succeeds and changes state,
            // violating the assertion.
        }
        other => panic!(
            "expected Counterexample on vulnerable fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

#[tokio::test]
async fn access_missing_modifier_clean_verifies() {
    let t = load_template("access-missing-modifier-state-change");
    let ctx = access_modifier_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("access-clean", &t.clean_source, &check);

    let result = run_simple(
        project.path(),
        "check_unauthorized_caller_cannot_mutate",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Verified { .. } => {
            // Expected: onlyOwner modifier reverts the attacker's call;
            // the catch branch is hit; the assertion is never reached.
        }
        other => panic!(
            "expected Verified on clean fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

// ─── arith-overflow-underflow-unchecked ──────────────────────────────────────

fn arith_overflow_render_ctx(attack_id_ident: &str) -> RenderContext {
    RenderContext::from_pairs([
        ("contract_name", "Target"),
        ("contract_path", "src/Target.sol"),
        ("op", "add"),
        ("attack_id_ident", attack_id_ident),
    ])
}

#[tokio::test]
async fn arith_overflow_vulnerable_produces_counterexample() {
    let t = load_template("arith-overflow-underflow-unchecked");
    let ctx = arith_overflow_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("arith-vuln", &t.vulnerable_source, &check);

    let result = run_simple(project.path(), "check_add_does_not_wrap", HALMOS_BUDGET).await;

    match result {
        HalmosResult::Counterexample { .. } => {
            // Expected: unchecked block wraps; result < a (or < b);
            // assertion fails.
        }
        other => panic!(
            "expected Counterexample on vulnerable fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

#[tokio::test]
async fn arith_overflow_clean_verifies() {
    let t = load_template("arith-overflow-underflow-unchecked");
    let ctx = arith_overflow_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("arith-clean", &t.clean_source, &check);

    let result = run_simple(project.path(), "check_add_does_not_wrap", HALMOS_BUDGET).await;

    match result {
        HalmosResult::Verified { .. } => {
            // Expected: checked arithmetic reverts on overflow;
            // post-revert paths are unreachable.
        }
        other => panic!(
            "expected Verified on clean fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

// ─── reentrancy-single-function-cei ──────────────────────────────────────────

fn reentrancy_render_ctx(attack_id_ident: &str) -> RenderContext {
    RenderContext::from_pairs([
        ("contract_name", "Target"),
        ("contract_path", "src/Target.sol"),
        ("action", "action"),
        ("getter", "counter"),
        ("attack_id_ident", attack_id_ident),
    ])
}

#[tokio::test]
async fn reentrancy_vulnerable_produces_counterexample() {
    let t = load_template("reentrancy-single-function-cei");
    let ctx = reentrancy_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("reentrancy-vuln", &t.vulnerable_source, &check);

    let result = run_simple(
        project.path(),
        "check_action_does_not_double_increment",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Counterexample { .. } => {
            // Expected: no guard, the receive() callback re-enters, counter
            // increments twice, assertion (c1 <= c0+1) fails.
        }
        other => panic!(
            "expected Counterexample on vulnerable reentrancy fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

#[tokio::test]
async fn reentrancy_clean_verifies() {
    let t = load_template("reentrancy-single-function-cei");
    let ctx = reentrancy_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("reentrancy-clean", &t.clean_source, &check);

    let result = run_simple(
        project.path(),
        "check_action_does_not_double_increment",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Verified { .. } => {
            // Expected: nonReentrant guard reverts re-entry; counter
            // increments at most once.
        }
        other => panic!(
            "expected Verified on clean reentrancy fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

// ─── access-public-burn-mint ─────────────────────────────────────────────────

fn mint_render_ctx(attack_id_ident: &str) -> RenderContext {
    RenderContext::from_pairs([
        ("contract_name", "Target"),
        ("contract_path", "src/Target.sol"),
        ("attack_id_ident", attack_id_ident),
    ])
}

#[tokio::test]
async fn public_burn_mint_vulnerable_produces_counterexample() {
    let t = load_template("access-public-burn-mint");
    let ctx = mint_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("mint-vuln", &t.vulnerable_source, &check);

    let result = run_simple(
        project.path(),
        "check_unauthorized_mint_cannot_inflate_supply",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Counterexample { .. } => {}
        other => panic!(
            "expected Counterexample on vulnerable mint fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

#[tokio::test]
async fn public_burn_mint_clean_verifies() {
    let t = load_template("access-public-burn-mint");
    let ctx = mint_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("mint-clean", &t.clean_source, &check);

    let result = run_simple(
        project.path(),
        "check_unauthorized_mint_cannot_inflate_supply",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Verified { .. } => {}
        other => panic!(
            "expected Verified on clean mint fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

// ─── init-unprotected-initializer ────────────────────────────────────────────

fn init_render_ctx(attack_id_ident: &str) -> RenderContext {
    RenderContext::from_pairs([
        ("contract_name", "Target"),
        ("contract_path", "src/Target.sol"),
        ("attack_id_ident", attack_id_ident),
    ])
}

#[tokio::test]
async fn init_unprotected_vulnerable_produces_counterexample() {
    let t = load_template("init-unprotected-initializer");
    let ctx = init_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("init-vuln", &t.vulnerable_source, &check);

    let result = run_simple(
        project.path(),
        "check_attacker_cannot_seize_ownership",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Counterexample { .. } => {}
        other => panic!(
            "expected Counterexample on vulnerable init fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

#[tokio::test]
async fn init_unprotected_clean_verifies() {
    let t = load_template("init-unprotected-initializer");
    let ctx = init_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("init-clean", &t.clean_source, &check);

    let result = run_simple(
        project.path(),
        "check_attacker_cannot_seize_ownership",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Verified { .. } => {}
        other => panic!(
            "expected Verified on clean init fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

// ─── logic-approval-not-revoked-after-cancel ─────────────────────────────────

fn hedgey_render_ctx(attack_id_ident: &str) -> RenderContext {
    RenderContext::from_pairs([
        ("contract_name", "Target"),
        ("contract_path", "src/Target.sol"),
        ("attack_id_ident", attack_id_ident),
    ])
}

#[tokio::test]
async fn approval_not_revoked_vulnerable_produces_counterexample() {
    let t = load_template("logic-approval-not-revoked-after-cancel");
    let ctx = hedgey_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("hedgey-allowance-vuln", &t.vulnerable_source, &check);

    let result = run_simple(
        project.path(),
        "check_cancel_zeros_allowance",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Counterexample { .. } => {}
        other => panic!(
            "expected Counterexample on vulnerable approval-not-revoked, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

#[tokio::test]
async fn approval_not_revoked_clean_verifies() {
    let t = load_template("logic-approval-not-revoked-after-cancel");
    let ctx = hedgey_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("hedgey-allowance-clean", &t.clean_source, &check);

    let result = run_simple(
        project.path(),
        "check_cancel_zeros_allowance",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Verified { .. } => {}
        other => panic!(
            "expected Verified on clean approval-not-revoked, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

// ─── input-missing-parameter-validation ──────────────────────────────────────

#[tokio::test]
async fn input_missing_validation_vulnerable_produces_counterexample() {
    let t = load_template("input-missing-parameter-validation");
    let ctx = hedgey_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("hedgey-input-vuln", &t.vulnerable_source, &check);

    let result = run_simple(
        project.path(),
        "check_attacker_cannot_cancel",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Counterexample { .. } => {}
        other => panic!(
            "expected Counterexample on vulnerable input-validation, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

#[tokio::test]
async fn input_missing_validation_clean_verifies() {
    let t = load_template("input-missing-parameter-validation");
    let ctx = hedgey_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("hedgey-input-clean", &t.clean_source, &check);

    let result = run_simple(
        project.path(),
        "check_attacker_cannot_cancel",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Verified { .. } => {}
        other => panic!(
            "expected Verified on clean input-validation, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

// ─── arith-incorrect-overflow-check-shift (Cetus BV demo) ────────────────────

fn shift_render_ctx(attack_id_ident: &str) -> RenderContext {
    RenderContext::from_pairs([
        ("contract_name", "Target"),
        ("contract_path", "src/Target.sol"),
        ("attack_id_ident", attack_id_ident),
    ])
}

#[tokio::test]
async fn incorrect_shift_vulnerable_produces_counterexample() {
    let t = load_template("arith-incorrect-overflow-check-shift");
    let ctx = shift_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("shift-vuln", &t.vulnerable_source, &check);

    let result = run_simple(project.path(), "check_shift_is_recoverable", HALMOS_BUDGET).await;

    match result {
        HalmosResult::Counterexample { .. } => {}
        other => panic!(
            "expected Counterexample on vulnerable shift fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

#[tokio::test]
async fn incorrect_shift_clean_verifies() {
    let t = load_template("arith-incorrect-overflow-check-shift");
    let ctx = shift_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("shift-clean", &t.clean_source, &check);

    let result = run_simple(project.path(), "check_shift_is_recoverable", HALMOS_BUDGET).await;

    match result {
        HalmosResult::Verified { .. } => {}
        other => panic!(
            "expected Verified on clean shift fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

// ─── vault-inflation-first-depositor-donation ────────────────────────────────

fn vault_render_ctx(attack_id_ident: &str) -> RenderContext {
    RenderContext::from_pairs([
        ("contract_name", "Target"),
        ("contract_path", "src/Target.sol"),
        ("attack_id_ident", attack_id_ident),
    ])
}

#[tokio::test]
async fn vault_inflation_vulnerable_produces_counterexample() {
    let t = load_template("vault-inflation-first-depositor-donation");
    let ctx = vault_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("vault-vuln", &t.vulnerable_source, &check);

    let result = run_simple(
        project.path(),
        "check_no_zero_shares_under_inflation",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Counterexample { .. } => {}
        other => panic!(
            "expected Counterexample on vulnerable vault fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

#[tokio::test]
async fn vault_inflation_clean_verifies() {
    let t = load_template("vault-inflation-first-depositor-donation");
    let ctx = vault_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("vault-clean", &t.clean_source, &check);

    let result = run_simple(
        project.path(),
        "check_no_zero_shares_under_inflation",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Verified { .. } => {}
        other => panic!(
            "expected Verified on clean vault fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

// ─── Phase 2 Slice 1: Category 1 batch ───────────────────────────────────────

fn cat1_simple_ctx(attack_id_ident: &str) -> RenderContext {
    RenderContext::from_pairs([
        ("contract_name", "Target"),
        ("contract_path", "src/Target.sol"),
        ("attack_id_ident", attack_id_ident),
    ])
}

async fn slice1_check(template_id: &str, check_fn: &str, label: &str, expect_cex: bool) {
    let t = load_template(template_id);
    let ctx = cat1_simple_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let src = if expect_cex {
        &t.vulnerable_source
    } else {
        &t.clean_source
    };
    let project = prepare_project(label, src, &check);
    let result = run_simple(project.path(), check_fn, HALMOS_BUDGET).await;
    match (expect_cex, result) {
        (true, HalmosResult::Counterexample { .. }) => {}
        (false, HalmosResult::Verified { .. }) => {}
        (expected, other) => panic!(
            "{template_id} expected {} got {other:?}\nrender dir: {}",
            if expected {
                "Counterexample"
            } else {
                "Verified"
            },
            project.path().display()
        ),
    }
}

#[tokio::test]
async fn unprotected_ether_withdrawal_vulnerable_cex() {
    slice1_check(
        "access-unprotected-ether-withdrawal",
        "check_unauthorized_caller_cannot_drain",
        "uew-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn unprotected_ether_withdrawal_clean_verified() {
    slice1_check(
        "access-unprotected-ether-withdrawal",
        "check_unauthorized_caller_cannot_drain",
        "uew-clean",
        false,
    )
    .await;
}

#[tokio::test]
async fn ownership_transfer_no_2step_vulnerable_cex() {
    slice1_check(
        "access-ownership-transfer-no-2step",
        "check_owner_not_stripped_in_single_step",
        "o2s-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn ownership_transfer_no_2step_clean_verified() {
    slice1_check(
        "access-ownership-transfer-no-2step",
        "check_owner_not_stripped_in_single_step",
        "o2s-clean",
        false,
    )
    .await;
}

#[tokio::test]
async fn role_escalation_self_grant_vulnerable_cex() {
    slice1_check(
        "access-role-escalation-self-grant",
        "check_attacker_cannot_self_grant",
        "res-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn role_escalation_self_grant_clean_verified() {
    slice1_check(
        "access-role-escalation-self-grant",
        "check_attacker_cannot_self_grant",
        "res-clean",
        false,
    )
    .await;
}

#[tokio::test]
async fn missing_zero_address_check_vulnerable_cex() {
    slice1_check(
        "access-missing-zero-address-check-admin",
        "check_admin_setter_rejects_zero_address",
        "zac-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn missing_zero_address_check_clean_verified() {
    slice1_check(
        "access-missing-zero-address-check-admin",
        "check_admin_setter_rejects_zero_address",
        "zac-clean",
        false,
    )
    .await;
}

#[tokio::test]
async fn unprotected_init_owner_setter_vulnerable_cex() {
    slice1_check(
        "access-unprotected-init-as-owner-setter",
        "check_attacker_cannot_seize_ownership",
        "uios-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn unprotected_init_owner_setter_clean_verified() {
    slice1_check(
        "access-unprotected-init-as-owner-setter",
        "check_attacker_cannot_seize_ownership",
        "uios-clean",
        false,
    )
    .await;
}

// access-tx-origin-auth deferred: Halmos symbolizes msg.sender/tx.origin for
// top-level check_ calls, so the phishing-proxy attack chain cannot be modeled
// with the bare-scaffold harness — tx.origin and msg.sender both become
// symbolic and the auth predicate doesn't decisively distinguish vulnerable
// from clean. Re-encode in V2 with a `--symbolic-msg-sender`-aware harness.

#[tokio::test]
async fn signature_auth_bypass_vulnerable_cex() {
    slice1_check(
        "access-signature-based-authorization-bypass",
        "check_invalid_signature_does_not_authorize",
        "sba-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn signature_auth_bypass_clean_verified() {
    slice1_check(
        "access-signature-based-authorization-bypass",
        "check_invalid_signature_does_not_authorize",
        "sba-clean",
        false,
    )
    .await;
}

// ─── init-uninitialized-uups-implementation (Wormhole class) ─────────────────

fn uups_render_ctx(attack_id_ident: &str) -> RenderContext {
    RenderContext::from_pairs([
        ("contract_name", "Target"),
        ("contract_path", "src/Target.sol"),
        ("attack_id_ident", attack_id_ident),
    ])
}

#[tokio::test]
async fn uups_uninitialized_vulnerable_produces_counterexample() {
    let t = load_template("init-uninitialized-uups-implementation");
    let ctx = uups_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("uups-vuln", &t.vulnerable_source, &check);

    let result = run_simple(
        project.path(),
        "check_implementation_cannot_be_initialized",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Counterexample { .. } => {}
        other => panic!(
            "expected Counterexample on vulnerable UUPS fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}

#[tokio::test]
async fn uups_uninitialized_clean_verifies() {
    let t = load_template("init-uninitialized-uups-implementation");
    let ctx = uups_render_ctx(&ident_for(&t.manifest.id));
    let check = render(&t.halmos_source, &ctx).expect("render halmos");
    let project = prepare_project("uups-clean", &t.clean_source, &check);

    let result = run_simple(
        project.path(),
        "check_implementation_cannot_be_initialized",
        HALMOS_BUDGET,
    )
    .await;

    match result {
        HalmosResult::Verified { .. } => {}
        other => panic!(
            "expected Verified on clean UUPS fixture, got {other:?}\n\
             render target dir: {}",
            project.path().display()
        ),
    }
}
