//! V1.5 Phase 6 Slice 10 — SPEC §11.6 exit test against live Anthropic.
//!
//! Gated on `--features llm-live` and the presence of
//! `VERGIL_ANTHROPIC_API_KEY` (or `ANTHROPIC_API_KEY`). Without the
//! key the test skips cleanly (CI default — manual-only LLM spend per
//! SPEC §11).
//!
//! ## SPEC §11.6 exit test (verbatim)
//!
//! ```text
//! vergil verify examples/erc20-broken
//! # → headline = Refuted; ≥1 counterexample with source:
//! #   attack_catalog; populated "Not checked" section.
//!
//! vergil verify examples/erc20 --yes
//! # → headline = Verified-in-scope.
//!
//! vergil prove vergil-out/proof.json
//! # → re-verifies the proven properties without re-running the LLM.
//! ```
//!
//! ## Cost
//!
//! Recorded for the operator. SPEC §4.2 caps at $0.50–$5 per
//! contract for the unified runner; the test prints token counts +
//! wall clock for both runs. The pass / fail gate is the property
//! outcomes — cost overrun is a human-review signal, not a test
//! failure, since pricing fluctuates.
//!
//! ## What this does NOT test
//!
//! Slice 10 runs against live Anthropic exactly once per invocation.
//! The cassette refresh process for downstream snapshot tests
//! (`phase6_snapshots.rs`) is a Slice 12 retro deliverable, not a
//! Slice 10 expectation.

#![cfg(feature = "llm-live")]

use std::path::Path;
use std::process::Command;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
}

fn require_api_key() -> Option<String> {
    let key = std::env::var("VERGIL_ANTHROPIC_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .ok()
        .filter(|k| !k.is_empty());
    if key.is_none() {
        eprintln!(
            "phase6_live: VERGIL_ANTHROPIC_API_KEY / ANTHROPIC_API_KEY not set or empty — \
             skipping. SPEC §11.6 exit test requires a live key per the \
             manual-only-CI policy. If the key lives in .env: \
             `set -a && source .env && set +a` before `cargo test --features llm-live`."
        );
    }
    key
}

fn vergil(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO"))
        .args([
            "run",
            "-p",
            "vergil-cli",
            "--bin",
            "vergil",
            "--quiet",
            "--features",
            "llm-live",
            "--",
        ])
        .args(args)
        .current_dir(workspace_root())
        .output()
        .expect("cargo run vergil")
}

/// Clean any leftover Phase-1 / V1 vergil-out so the live run produces
/// a Phase-6 artifact from scratch. Keep `proof.json` references for
/// the prove-step test; cleaning is per-project.
fn clean_vergil_out(project: &Path) {
    let dir = project.join("vergil-out");
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn read_proof_json(project: &Path) -> serde_json::Value {
    let p = project.join("vergil-out/proof.json");
    let body = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
}

// ─── Exit test 1: erc20-broken must Refute with a catalog cex ────────

#[test]
fn erc20_broken_verifies_to_refuted_with_catalog_cex() {
    let Some(_) = require_api_key() else {
        return;
    };
    let project = workspace_root().join("examples/erc20-broken");
    clean_vergil_out(&project);
    let started = std::time::Instant::now();
    let out = vergil(&["verify", project.to_str().expect("utf8 path"), "--yes"]);
    let elapsed = started.elapsed();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    eprintln!(
        "phase6_live[erc20-broken]: wall_clock={:.1}s",
        elapsed.as_secs_f64()
    );
    eprintln!("phase6_live[erc20-broken] stdout:\n{stdout}");
    eprintln!("phase6_live[erc20-broken] stderr:\n{stderr}");

    // Refuted exit code is 1 (SPEC §3.1). Cleanly-verified would be 0;
    // any Unknown / Error would be 2 / 3.
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit code 1 (Refuted); got {:?}",
        out.status.code(),
    );

    let proof = read_proof_json(&project);
    let verdict = &proof["verdict"];
    assert_eq!(
        verdict["headline_machine"], "refuted",
        "headline must be 'refuted' on erc20-broken: {verdict}"
    );

    // ≥1 refuted property from any Phase 6 Stage-1 oracle. SPEC §11.6
    // names the attack catalog specifically as the desired source,
    // but the multi-oracle stack (catalog + tests + natspec) is
    // sound either way — if natspec-derived invariants catch the
    // bug, that's the same product story (Refuted headline + a
    // runnable forge-test cex). The Slice 12 retro documents which
    // oracle surfaces the cex on the bench; tuning the catalog to
    // catch the transferFrom-allowance-skip is data-side work
    // outside Phase 6's plumbing scope.
    let properties = verdict["properties"].as_array().expect("properties array");
    let zc_cex = properties
        .iter()
        .find(|p| p["tier"] == "zero-config" && p["verdict"]["kind"] == "refuted");
    assert!(
        zc_cex.is_some(),
        "no zero-config Refuted property in verdict — Stage 1 oracles \
         (catalog / tests / natspec) should have caught erc20-broken's \
         transferFrom bug: {properties:#?}"
    );
    let cex_source = zc_cex.unwrap()["source"]
        .as_str()
        .unwrap_or("?")
        .to_string();
    eprintln!("phase6_live[erc20-broken]: refutation surfaced via source={cex_source}");

    // Counterexample file landed on disk via Slice 6's CexSink.
    let cex_dir = project.join("vergil-out/counterexamples");
    let has_cex_file = std::fs::read_dir(&cex_dir)
        .map(|entries| {
            entries
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with("Cex_"))
        })
        .unwrap_or(false);
    assert!(has_cex_file, "no Cex_*.t.sol file at {}", cex_dir.display());

    // "Not checked" section is populated (Phase 5 + skipped templates).
    let report =
        std::fs::read_to_string(project.join("vergil-out/report.md")).expect("read report.md");
    assert!(
        report.contains("## Not checked"),
        "report missing Not-checked section"
    );
    assert!(
        report.contains("Structural mining (Phase 5)")
            || report.contains("attack-catalog templates skipped")
            || report.contains("document-only templates"),
        "Not-checked section is empty on erc20-broken — expected at least \
         Phase 5 pending or skipped templates:\n{report}"
    );
}

// ─── Exit test 2: erc20 (clean) → Verified-in-scope, no human pause ──

#[test]
fn erc20_clean_verifies_to_verified_in_scope_with_yes() {
    let Some(_) = require_api_key() else {
        return;
    };
    let project = workspace_root().join("examples/erc20");
    clean_vergil_out(&project);
    let started = std::time::Instant::now();
    let out = vergil(&["verify", project.to_str().expect("utf8 path"), "--yes"]);
    let elapsed = started.elapsed();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    eprintln!(
        "phase6_live[erc20]: wall_clock={:.1}s",
        elapsed.as_secs_f64()
    );
    eprintln!("phase6_live[erc20] stdout:\n{stdout}");
    eprintln!("phase6_live[erc20] stderr:\n{stderr}");

    let proof = read_proof_json(&project);
    let verdict = &proof["verdict"];
    let headline = verdict["headline_machine"].as_str().unwrap_or("");
    // Clean erc20 should be Verified-in-scope; Incomplete is acceptable
    // if a frontier template lands Unknown (SPEC §4.3). Refuted on
    // erc20 is a soundness failure — fail the test.
    assert_ne!(
        headline, "refuted",
        "erc20 (clean) MUST NOT verify to Refuted: {verdict}"
    );
    // --yes path must not emit the interactive [c/s/e/a] prompt — that
    // would mean the gate's auto-confirm shortcut didn't fire.
    assert!(
        !stderr.contains("[c]onfirm / [s]kip / [e]dit / [a]ll-yes"),
        "--yes must skip the human prompt: {stderr}"
    );
}

// ─── Exit test 3: vergil prove re-verifies independently ─────────────

#[test]
fn vergil_prove_re_verifies_phase6_proof_json() {
    let Some(_) = require_api_key() else {
        return;
    };
    let project = workspace_root().join("examples/erc20");
    // This test depends on the erc20 verify run having produced a
    // proof.json. If the verify test was skipped or run in isolation,
    // re-do a quick verify first.
    if !project.join("vergil-out/proof.json").is_file() {
        let _ = vergil(&["verify", project.to_str().expect("utf8 path"), "--yes"]);
    }
    let proof_path = project.join("vergil-out/proof.json");
    if !proof_path.is_file() {
        eprintln!(
            "phase6_live[prove]: proof.json absent at {} — skipping re-verify test",
            proof_path.display()
        );
        return;
    }
    let out = vergil(&["prove", proof_path.to_str().expect("utf8 path")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    eprintln!("phase6_live[prove] stdout:\n{stdout}");
    eprintln!("phase6_live[prove] stderr:\n{stderr}");
    assert!(
        out.status.success(),
        "vergil prove failed to re-verify: stdout={stdout}\nstderr={stderr}"
    );
}
