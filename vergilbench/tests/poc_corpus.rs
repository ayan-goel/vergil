//! Held-out PoC corpus — SPEC §11.2 kill criterion's zero-FN gate.
//!
//! For each historical exploit reproduction under `vergilbench/poc-corpus/`:
//!   1. Load the per-PoC `expected.yaml` declaring the catalog template
//!      that MUST refute the reproduction.
//!   2. Load that template from the shipped attack catalog.
//!   3. Render the template's Halmos encoding with the PoC's bindings.
//!   4. Drop the rendered check + the PoC's `src/Vulnerable.sol` into
//!      a temp Foundry project.
//!   5. Run Halmos and assert the result is `Counterexample` (the
//!      template DID detect the bug) — anything else is a false
//!      negative.
//!
//! This is gated on `--features integration` because each PoC compiles
//! solc and spawns Halmos.

#![cfg(feature = "integration")]

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use vergil_properties::{render, AttackCatalog, RenderContext};
use vergil_solidity::halmos::{run_with_args, HalmosResult};

const HALMOS_BUDGET: Duration = Duration::from_secs(120);

const FOUNDRY_TOML: &str = r#"[profile.default]
src = "src"
test = "test"
out = "out"
libs = ["lib"]
solc = "0.8.20"
optimizer = true
optimizer_runs = 200
"#;

#[derive(Debug, Deserialize)]
struct ExpectedYaml {
    incident: Incident,
    template: TemplateRef,
    #[serde(default)]
    #[allow(dead_code)]
    provenance: Option<Provenance>,
}

#[derive(Debug, Deserialize)]
struct Incident {
    name: String,
    year: u32,
    #[serde(default)]
    #[allow(dead_code)]
    loss_usd_approx: Option<u64>,
    #[serde(default)]
    #[allow(dead_code)]
    chain: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    reference_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TemplateRef {
    id: String,
    check_fn: String,
    #[serde(default)]
    bindings: std::collections::BTreeMap<String, String>,
    /// Extra CLI flags to forward to Halmos. Used for PoCs whose
    /// templates need e.g. `--symbolic-msg-sender`.
    #[serde(default)]
    halmos_extra_args: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Provenance {
    #[allow(dead_code)]
    source: String,
    #[allow(dead_code)]
    #[serde(default)]
    attribution: Option<String>,
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("poc-corpus")
}

fn templates_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("crates")
        .join("vergil-properties")
        .join("templates")
        .join("attacks")
}

fn write(p: &Path, body: &str) {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, body).unwrap();
}

fn prepare_project(label: &str, vulnerable_src: &str, check_src: &str) -> tempfile::TempDir {
    let tmp = tempfile::Builder::new()
        .prefix(&format!("vergil-poc-{label}-"))
        .tempdir()
        .expect("tempdir");
    let root = tmp.path();
    write(&root.join("foundry.toml"), FOUNDRY_TOML);
    write(&root.join("src/Vulnerable.sol"), vulnerable_src);
    write(&root.join("test/AttackCheck.t.sol"), check_src);
    tmp
}

/// Validate a single PoC under `poc-corpus/<incident-id>/`.
async fn validate_poc(incident_id: &str) {
    let dir = corpus_dir().join(incident_id);
    assert!(
        dir.exists(),
        "PoC directory {} missing — expected vergilbench/poc-corpus/{}/",
        dir.display(),
        incident_id
    );

    let expected_path = dir.join("expected.yaml");
    let expected_bytes = std::fs::read(&expected_path).unwrap_or_else(|e| {
        panic!("read {}: {e}", expected_path.display());
    });
    let expected: ExpectedYaml = serde_yaml::from_slice(&expected_bytes)
        .unwrap_or_else(|e| panic!("parse {}: {e}", expected_path.display()));

    let vulnerable_path = dir.join("src/Vulnerable.sol");
    let vulnerable_src = std::fs::read_to_string(&vulnerable_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", vulnerable_path.display()));

    let catalog = AttackCatalog::load(templates_dir()).expect("attack catalog loads");
    let template = catalog.get(&expected.template.id).unwrap_or_else(|| {
        panic!(
            "PoC '{incident_id}' references template '{}' which is not in the catalog",
            expected.template.id
        )
    });

    let mut ctx = RenderContext::default();
    for (k, v) in &expected.template.bindings {
        ctx.insert(k, v);
    }
    // Always-present binding: the attack-id identifier used by the
    // Halmos template's `Check_<id>` contract name. The integration
    // suite computes this from the template id; we do the same so PoC
    // YAML doesn't have to repeat it.
    if !expected.template.bindings.contains_key("attack_id_ident") {
        ctx.insert("attack_id_ident", template.manifest.id.replace('-', "_"));
    }

    let rendered = render(&template.halmos_source, &ctx).expect("render PoC check");

    let label = incident_id.replace('-', "_");
    let project = prepare_project(&label, &vulnerable_src, &rendered);

    let extra_args: Vec<String> = expected.template.halmos_extra_args.clone();
    let result = run_with_args(
        project.path(),
        &expected.template.check_fn,
        HALMOS_BUDGET,
        extra_args,
    )
    .await;

    match result {
        HalmosResult::Counterexample { .. } => {
            // Pass: the template detected the bug. Zero-FN gate met for this PoC.
        }
        HalmosResult::Verified { .. } => {
            panic!(
                "FALSE NEGATIVE on PoC '{incident_id}' ({} {}): template '{}' \
                 returned Verified — it failed to detect the historical bug.\n\
                 Render dir: {}\nThis breaches SPEC §11.2's zero-false-negative kill criterion.",
                expected.incident.name,
                expected.incident.year,
                expected.template.id,
                project.path().display()
            );
        }
        other => {
            panic!(
                "PoC '{incident_id}' ({} {}) on template '{}' returned {other:?} \
                 (expected Counterexample).\nRender dir: {}",
                expected.incident.name,
                expected.incident.year,
                expected.template.id,
                project.path().display()
            );
        }
    }
}

// ─── PoC test cases ──────────────────────────────────────────────────────────

#[tokio::test]
async fn poc_the_dao_2016() {
    validate_poc("the-dao-2016").await;
}

#[tokio::test]
async fn poc_beautychain_2018() {
    validate_poc("beautychain-2018").await;
}

#[tokio::test]
async fn poc_audius_2022() {
    validate_poc("audius-2022").await;
}

#[tokio::test]
async fn poc_wormhole_2022() {
    validate_poc("wormhole-2022").await;
}

#[tokio::test]
async fn poc_cream_finance_2021() {
    validate_poc("cream-finance-2021").await;
}

#[tokio::test]
async fn poc_cetus_2024() {
    validate_poc("cetus-2024").await;
}

#[tokio::test]
async fn poc_hedgey_2024() {
    validate_poc("hedgey-2024").await;
}

#[tokio::test]
async fn poc_beanstalk_2022() {
    validate_poc("beanstalk-2022").await;
}

#[tokio::test]
async fn poc_imbtc_uniswap_v1_2020() {
    validate_poc("imbtc-uniswap-v1-2020").await;
}

#[tokio::test]
async fn poc_king_of_ether_2016() {
    validate_poc("king-of-ether-2016").await;
}
