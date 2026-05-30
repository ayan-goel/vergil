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
