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

// ─── Phase 2 Slice 2: Category 2 batch (Init & Proxy) ────────────────────────

#[tokio::test]
async fn missing_constructor_disable_init_vulnerable_cex() {
    slice1_check(
        "proxy-missing-constructor-disable-init",
        "check_implementation_cannot_be_initialized",
        "mcdi-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn missing_constructor_disable_init_clean_verified() {
    slice1_check(
        "proxy-missing-constructor-disable-init",
        "check_implementation_cannot_be_initialized",
        "mcdi-clean",
        false,
    )
    .await;
}

#[tokio::test]
async fn reinitialization_after_upgrade_vulnerable_cex() {
    slice1_check(
        "init-reinitialization-after-upgrade",
        "check_migrate_does_not_unlock_initializer",
        "riu-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn reinitialization_after_upgrade_clean_verified() {
    slice1_check(
        "init-reinitialization-after-upgrade",
        "check_migrate_does_not_unlock_initializer",
        "riu-clean",
        false,
    )
    .await;
}

#[tokio::test]
async fn unprotected_upgrade_function_vulnerable_cex() {
    slice1_check(
        "proxy-unprotected-upgrade-function",
        "check_unauthorized_caller_cannot_upgrade",
        "uuf-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn unprotected_upgrade_function_clean_verified() {
    slice1_check(
        "proxy-unprotected-upgrade-function",
        "check_unauthorized_caller_cannot_upgrade",
        "uuf-clean",
        false,
    )
    .await;
}

#[tokio::test]
async fn selfdestruct_in_logic_vulnerable_cex() {
    slice1_check(
        "proxy-selfdestruct-in-logic",
        "check_unauthorized_caller_cannot_destruct",
        "sdl-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn selfdestruct_in_logic_clean_verified() {
    slice1_check(
        "proxy-selfdestruct-in-logic",
        "check_unauthorized_caller_cannot_destruct",
        "sdl-clean",
        false,
    )
    .await;
}

#[tokio::test]
async fn double_initialization_vulnerable_cex() {
    slice1_check(
        "init-double-initialization",
        "check_attacker_cannot_seize_ownership",
        "dinit-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn double_initialization_clean_verified() {
    slice1_check(
        "init-double-initialization",
        "check_attacker_cannot_seize_ownership",
        "dinit-clean",
        false,
    )
    .await;
}

#[tokio::test]
async fn function_selector_clash_vulnerable_cex() {
    slice1_check(
        "proxy-function-selector-clash",
        "check_admin_selector_protected",
        "fsc-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn function_selector_clash_clean_verified() {
    slice1_check(
        "proxy-function-selector-clash",
        "check_admin_selector_protected",
        "fsc-clean",
        false,
    )
    .await;
}

// ─── Phase 2 Slice 3: Category 3 batch (Arithmetic) ──────────────────────────

#[tokio::test]
async fn downcast_truncation_vulnerable_cex() {
    slice1_check(
        "arith-truncation-cast-downcast",
        "check_downcast_is_lossless",
        "dcast-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn downcast_truncation_clean_verified() {
    slice1_check(
        "arith-truncation-cast-downcast",
        "check_downcast_is_lossless",
        "dcast-clean",
        false,
    )
    .await;
}

#[tokio::test]
async fn precision_loss_divide_before_multiply_vulnerable_cex() {
    slice1_check(
        "arith-precision-loss-divide-before-multiply",
        "check_no_precision_loss",
        "pldbm-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn precision_loss_divide_before_multiply_clean_verified() {
    slice1_check(
        "arith-precision-loss-divide-before-multiply",
        "check_no_precision_loss",
        "pldbm-clean",
        false,
    )
    .await;
}

#[tokio::test]
async fn fee_calc_rounding_zero_vulnerable_cex() {
    slice1_check(
        "arith-fee-calc-rounding-zero",
        "check_fee_positive_for_positive_amount",
        "fee-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn fee_calc_rounding_zero_clean_verified() {
    slice1_check(
        "arith-fee-calc-rounding-zero",
        "check_fee_positive_for_positive_amount",
        "fee-clean",
        false,
    )
    .await;
}

// ─── Phase 2 Slice 5: Category 8 batch (Input Validation) ────────────────────

#[tokio::test]
async fn zero_address_check_vulnerable_cex() {
    slice1_check(
        "input-zero-address-check",
        "check_register_rejects_zero_address",
        "izac-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn zero_address_check_clean_verified() {
    slice1_check(
        "input-zero-address-check",
        "check_register_rejects_zero_address",
        "izac-clean",
        false,
    )
    .await;
}

// input-array-length-mismatch deferred: Solidity 0.8+ OOB panic on the
// short-array branch makes vulnerable and clean both revert from the
// caller's view, so Halmos can't decisively distinguish. Re-encode with
// a partial-application model in a focused later slice.

#[tokio::test]
async fn deadline_future_vulnerable_cex() {
    slice1_check(
        "input-time-bounds-deadline-future",
        "check_executor_rejects_past_deadline",
        "dlf-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn deadline_future_clean_verified() {
    slice1_check(
        "input-time-bounds-deadline-future",
        "check_executor_rejects_past_deadline",
        "dlf-clean",
        false,
    )
    .await;
}

#[tokio::test]
async fn amount_bounded_by_balance_vulnerable_cex() {
    slice1_check(
        "input-amount-bounded-by-balance",
        "check_overdraft_rejected",
        "abb-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn amount_bounded_by_balance_clean_verified() {
    slice1_check(
        "input-amount-bounded-by-balance",
        "check_overdraft_rejected",
        "abb-clean",
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

// ─── Phase 2 Slice 4: Categories 4 + 5 batch (Reentrancy + Vault) ────────────

// All Slice 4 templates use the same simple ctx shape as Slice 1, so the
// existing `slice1_check` helper applies directly.

// 4.2 reentrancy-cross-function-state
#[tokio::test]
async fn reentrancy_cross_function_vulnerable_cex() {
    slice1_check(
        "reentrancy-cross-function-state",
        "check_cross_function_does_not_double_increment",
        "rxf-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn reentrancy_cross_function_clean_verified() {
    slice1_check(
        "reentrancy-cross-function-state",
        "check_cross_function_does_not_double_increment",
        "rxf-clean",
        false,
    )
    .await;
}

// 4.3 reentrancy-callback-token-hook
#[tokio::test]
async fn reentrancy_token_hook_vulnerable_cex() {
    slice1_check(
        "reentrancy-callback-token-hook",
        "check_hook_does_not_double_transfer",
        "rth-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn reentrancy_token_hook_clean_verified() {
    slice1_check(
        "reentrancy-callback-token-hook",
        "check_hook_does_not_double_transfer",
        "rth-clean",
        false,
    )
    .await;
}

// 4.5 reentrancy-eth-transfer-after-state
#[tokio::test]
async fn reentrancy_eth_after_state_vulnerable_cex() {
    slice1_check(
        "reentrancy-eth-transfer-after-state",
        "check_no_double_drain",
        "etas-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn reentrancy_eth_after_state_clean_verified() {
    slice1_check(
        "reentrancy-eth-transfer-after-state",
        "check_no_double_drain",
        "etas-clean",
        false,
    )
    .await;
}

// 5.2 vault-zero-share-deposit
#[tokio::test]
async fn vault_zero_share_vulnerable_cex() {
    slice1_check(
        "vault-zero-share-deposit",
        "check_positive_deposit_mints_positive_shares",
        "vzs-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn vault_zero_share_clean_verified() {
    slice1_check(
        "vault-zero-share-deposit",
        "check_positive_deposit_mints_positive_shares",
        "vzs-clean",
        false,
    )
    .await;
}

// 5.3 vault-totalsupply-totalassets-invariant
#[tokio::test]
async fn vault_supply_invariant_vulnerable_cex() {
    slice1_check(
        "vault-totalsupply-totalassets-invariant",
        "check_supply_matches_per_user_balance",
        "vsi-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn vault_supply_invariant_clean_verified() {
    slice1_check(
        "vault-totalsupply-totalassets-invariant",
        "check_supply_matches_per_user_balance",
        "vsi-clean",
        false,
    )
    .await;
}

// 5.4 vault-convertToShares-monotonicity
#[tokio::test]
async fn vault_monotone_vulnerable_cex() {
    slice1_check(
        "vault-convertToShares-monotonicity",
        "check_convert_to_shares_monotone",
        "vmn-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn vault_monotone_clean_verified() {
    slice1_check(
        "vault-convertToShares-monotonicity",
        "check_convert_to_shares_monotone",
        "vmn-clean",
        false,
    )
    .await;
}

// 5.5 vault-redeem-more-than-deposited
#[tokio::test]
async fn vault_redeem_overage_vulnerable_cex() {
    slice1_check(
        "vault-redeem-more-than-deposited",
        "check_redeem_more_than_owned_reverts",
        "vro-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn vault_redeem_overage_clean_verified() {
    slice1_check(
        "vault-redeem-more-than-deposited",
        "check_redeem_more_than_owned_reverts",
        "vro-clean",
        false,
    )
    .await;
}

// 5.6 vault-donation-exchange-rate-manipulation
#[tokio::test]
async fn vault_donation_rate_vulnerable_cex() {
    slice1_check(
        "vault-donation-exchange-rate-manipulation",
        "check_rate_stable_under_donation",
        "vdr-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn vault_donation_rate_clean_verified() {
    slice1_check(
        "vault-donation-exchange-rate-manipulation",
        "check_rate_stable_under_donation",
        "vdr-clean",
        false,
    )
    .await;
}

// ─── Phase 2 Slice 6: Categories 9 + 10 + 11 batch ───────────────────────────

// All Slice 6 templates use the same simple ctx shape as Slice 1.

// 9.1 token-fee-on-transfer-balance-drift
#[tokio::test]
async fn token_fot_drift_vulnerable_cex() {
    slice1_check(
        "token-fee-on-transfer-balance-drift",
        "check_internal_credit_bounded_by_received",
        "fot-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn token_fot_drift_clean_verified() {
    slice1_check(
        "token-fee-on-transfer-balance-drift",
        "check_internal_credit_bounded_by_received",
        "fot-clean",
        false,
    )
    .await;
}

// 10.1 sig-missing-nonce
#[tokio::test]
async fn sig_missing_nonce_vulnerable_cex() {
    slice1_check(
        "sig-missing-nonce",
        "check_proof_cannot_be_replayed",
        "smn-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn sig_missing_nonce_clean_verified() {
    slice1_check(
        "sig-missing-nonce",
        "check_proof_cannot_be_replayed",
        "smn-clean",
        false,
    )
    .await;
}

// 10.3 sig-ecrecover-zero-address-bypass
#[tokio::test]
async fn sig_zero_addr_vulnerable_cex() {
    slice1_check(
        "sig-ecrecover-zero-address-bypass",
        "check_zero_address_does_not_authorize",
        "sza-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn sig_zero_addr_clean_verified() {
    slice1_check(
        "sig-ecrecover-zero-address-bypass",
        "check_zero_address_does_not_authorize",
        "sza-clean",
        false,
    )
    .await;
}

// 11.2 selfdestruct-arbitrary-beneficiary
#[tokio::test]
async fn selfdestruct_arbitrary_vulnerable_cex() {
    slice1_check(
        "selfdestruct-arbitrary-beneficiary",
        "check_non_owner_cannot_destroy",
        "sda-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn selfdestruct_arbitrary_clean_verified() {
    slice1_check(
        "selfdestruct-arbitrary-beneficiary",
        "check_non_owner_cannot_destroy",
        "sda-clean",
        false,
    )
    .await;
}

// 11.3 lowlevel-delegatecall-untrusted
#[tokio::test]
async fn delegatecall_untrusted_vulnerable_cex() {
    slice1_check(
        "lowlevel-delegatecall-untrusted",
        "check_attacker_cannot_seize_via_delegatecall",
        "dcu-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn delegatecall_untrusted_clean_verified() {
    slice1_check(
        "lowlevel-delegatecall-untrusted",
        "check_attacker_cannot_seize_via_delegatecall",
        "dcu-clean",
        false,
    )
    .await;
}

// 11.5 lowlevel-call-return-ignored
#[tokio::test]
async fn call_return_ignored_vulnerable_cex() {
    slice1_check(
        "lowlevel-call-return-ignored",
        "check_failed_call_does_not_credit",
        "cri-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn call_return_ignored_clean_verified() {
    slice1_check(
        "lowlevel-call-return-ignored",
        "check_failed_call_does_not_credit",
        "cri-clean",
        false,
    )
    .await;
}

// ─── Phase 2 Slice 7: Categories 12 + 13 + 14 batch ──────────────────────────

// 12.2 oracle-missing-staleness-check
#[tokio::test]
async fn oracle_staleness_vulnerable_cex() {
    slice1_check(
        "oracle-missing-staleness-check",
        "check_stale_price_rejected",
        "oms-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn oracle_staleness_clean_verified() {
    slice1_check(
        "oracle-missing-staleness-check",
        "check_stale_price_rejected",
        "oms-clean",
        false,
    )
    .await;
}

// 13.1 flashloan-balance-dependent-state
#[tokio::test]
async fn flashloan_balance_dep_vulnerable_cex() {
    slice1_check(
        "flashloan-balance-dependent-state",
        "check_flash_loaned_balance_does_not_authorize",
        "fbd-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn flashloan_balance_dep_clean_verified() {
    slice1_check(
        "flashloan-balance-dependent-state",
        "check_flash_loaned_balance_does_not_authorize",
        "fbd-clean",
        false,
    )
    .await;
}

// 13.2 flashloan-no-amount-validation
#[tokio::test]
async fn flashloan_no_cap_vulnerable_cex() {
    slice1_check(
        "flashloan-no-amount-validation",
        "check_loan_above_cap_reverts",
        "fnv-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn flashloan_no_cap_clean_verified() {
    slice1_check(
        "flashloan-no-amount-validation",
        "check_loan_above_cap_reverts",
        "fnv-clean",
        false,
    )
    .await;
}

// 13.3 flashloan-share-inflation-sensitive
#[tokio::test]
async fn flashloan_share_inflation_vulnerable_cex() {
    slice1_check(
        "flashloan-share-inflation-sensitive",
        "check_rate_stable_under_inflation",
        "fsi-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn flashloan_share_inflation_clean_verified() {
    slice1_check(
        "flashloan-share-inflation-sensitive",
        "check_rate_stable_under_inflation",
        "fsi-clean",
        false,
    )
    .await;
}

// 14.3 quirk-abi-encode-packed-collision
#[tokio::test]
async fn quirk_packed_collision_vulnerable_cex() {
    slice1_check(
        "quirk-abi-encode-packed-collision",
        "check_distinct_inputs_distinct_hashes",
        "qpc-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn quirk_packed_collision_clean_verified() {
    slice1_check(
        "quirk-abi-encode-packed-collision",
        "check_distinct_inputs_distinct_hashes",
        "qpc-clean",
        false,
    )
    .await;
}

// ─── Phase 2 Slice 9: Categories 15 + 16 batch ───────────────────────────────
// 15.1 eip7702-delegate-arbitrary-execution ships as document-only —
// no integration test; the load path is exercised by the lib unit test
// `loads_document_only_template_without_encoding_or_fixtures`.

// 16.1 dos-unbounded-loop-user-array
#[tokio::test]
async fn dos_unbounded_loop_vulnerable_cex() {
    slice1_check(
        "dos-unbounded-loop-user-array",
        "check_oversized_array_rejected",
        "dul-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn dos_unbounded_loop_clean_verified() {
    slice1_check(
        "dos-unbounded-loop-user-array",
        "check_oversized_array_rejected",
        "dul-clean",
        false,
    )
    .await;
}

// 16.2 dos-push-payment-failure
#[tokio::test]
async fn dos_push_payment_vulnerable_cex() {
    slice1_check(
        "dos-push-payment-failure",
        "check_one_recipient_revert_does_not_halt_distribution",
        "dpp-vuln",
        true,
    )
    .await;
}
#[tokio::test]
async fn dos_push_payment_clean_verified() {
    slice1_check(
        "dos-push-payment-failure",
        "check_one_recipient_revert_does_not_halt_distribution",
        "dpp-clean",
        false,
    )
    .await;
}
