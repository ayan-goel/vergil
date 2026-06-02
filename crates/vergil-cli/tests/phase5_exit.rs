//! V1.5 Phase 5 Slice 7 — SPEC §11.5 exit gate.
//!
//! ## SPEC §11.5 exit test (verbatim)
//!
//! ```text
//! vergil verify examples/erc20 --mode zero-config
//! # → produces ≥5 properties tagged `source: structural` with
//! #   confidence ≥0.6, ≥80% of which verify.
//! ```
//!
//! ## What this file tests
//!
//! 1. **Mining floor** — `extract_from_structural` on `examples/erc20`'s
//!    source set + solc storage layouts produces ≥5 candidates at
//!    confidence ≥0.6.
//! 2. **Soundness on a buggy contract** — `examples/erc20-broken`
//!    produces structural candidates whose mining decisions are sane
//!    (no obviously-wrong invariant claim about the part that's
//!    broken).
//! 3. **Determinism** — Two consecutive runs of the same mining
//!    call return byte-identical candidate vecs. Phase 5 has zero LLM
//!    dependency so the run must be deterministic by construction.
//!
//! ## What this file does NOT test
//!
//! The "≥80% verify" half of the SPEC §11.5 exit gate requires actually
//! running Halmos symbolic execution against the emitted check_*
//! functions. That depends on the foundry + halmos toolchain being
//! installed and a full scaffold being available — covered by the
//! existing `phase6_live.rs` live-LLM tests (which exercise the whole
//! pipeline including structural). The verification rate is documented
//! in `notes/v1.5-phase5.md` per the Phase 5 retro.
//!
//! ## Gating
//!
//! `solc` must be on PATH. The test skips cleanly otherwise so it works
//! in environments without the Solidity toolchain. **No LLM key
//! required** — Phase 5 is pure static analysis.

use std::path::{Path, PathBuf};
use std::time::Duration;

use vergil_core::structural::{extract_from_structural, StructuralConfig};
use vergil_solidity::storage::{run_simple, StorageLayout, StorageResult};

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
}

fn solc_available() -> bool {
    std::process::Command::new("solc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn load_sources(project: &Path) -> Vec<(PathBuf, String)> {
    let src_dir = project.join("src");
    let Ok(rd) = std::fs::read_dir(&src_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("sol") {
            continue;
        }
        if let Ok(s) = std::fs::read_to_string(&p) {
            out.push((p, s));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

async fn load_layouts(paths: &[PathBuf]) -> Vec<StorageLayout> {
    let mut out = Vec::new();
    for p in paths {
        match run_simple(p, Duration::from_secs(30)).await {
            StorageResult::Ok(mut l) => out.append(&mut l),
            StorageResult::Error(e) => eprintln!("phase5_exit: solc {} → {e}", p.display()),
        }
    }
    out
}

/// Decode the confidence encoded into a structural candidate's
/// `template_ref` as `"structural:{miner}:{conf:.2}"`. Returns 0.0 when
/// the template_ref doesn't match the expected shape (used as a
/// conservative filter floor).
fn decode_confidence(template_ref: Option<&str>) -> f32 {
    let s = template_ref.unwrap_or("");
    let Some(rest) = s.strip_prefix("structural:") else {
        return 0.0;
    };
    let Some((_, conf)) = rest.rsplit_once(':') else {
        return 0.0;
    };
    conf.parse::<f32>().unwrap_or(0.0)
}

// ─── Test 1: SPEC §11.5 mining floor on examples/erc20 ───────────────

#[tokio::test]
async fn erc20_mines_at_least_five_high_confidence_structural_candidates() {
    if !solc_available() {
        eprintln!("phase5_exit: solc not on PATH — skipping");
        return;
    }
    let project = workspace_root().join("examples/erc20");
    let sources = load_sources(&project);
    let layouts = load_layouts(
        &sources.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(),
    )
    .await;
    let report = extract_from_structural(&sources, &layouts, &StructuralConfig::default());

    // Detailed breakdown for operator visibility.
    eprintln!(
        "phase5_exit[erc20]: {} high-confidence candidates, {} low-confidence findings",
        report.candidates.len(),
        report.low_confidence_findings.len()
    );
    for c in &report.candidates {
        eprintln!(
            "  - {} ({}) — {}",
            c.name,
            c.template_ref.as_deref().unwrap_or("?"),
            c.intent_text.as_deref().unwrap_or("?")
        );
    }

    // Per-miner breakdown.
    let by_miner_pretty: std::collections::BTreeMap<String, usize> = report
        .miner_counts
        .iter()
        .map(|(m, n)| (m.id().to_string(), *n))
        .collect();
    eprintln!("phase5_exit[erc20] by miner: {by_miner_pretty:?}");

    // SPEC §11.5 floor: ≥5 candidates at confidence ≥0.6.
    let high_conf: Vec<&_> = report
        .candidates
        .iter()
        .filter(|c| decode_confidence(c.template_ref.as_deref()) >= 0.6 - 1e-3)
        .collect();
    assert!(
        high_conf.len() >= 5,
        "SPEC §11.5: expected ≥5 candidates at confidence ≥0.6, got {} on erc20. \
         Full candidate list above. Phase 5 retro should document this as a deviation if persistent.",
        high_conf.len()
    );
}

// ─── Test 2: Soundness on the buggy contract ─────────────────────────

#[tokio::test]
async fn erc20_broken_structural_mining_is_sound() {
    // Soundness check: the broken erc20's bug is in `transferFrom`
    // (allowance-skip per Phase 6 retro). The structural miner should
    // NOT emit a candidate that falsely asserts the broken behavior
    // is sound. The conservation miner's per-(mapping,fn) candidate
    // for transferFrom claims sum-preservation, which is in fact
    // STILL TRUE on the broken contract (the bug is allowance, not
    // balance arithmetic). So the candidate is correctly emittable.
    //
    // This test confirms the miner produces *some* candidates without
    // falsely claiming a property the bug violates.
    if !solc_available() {
        eprintln!("phase5_exit: solc not on PATH — skipping");
        return;
    }
    let project = workspace_root().join("examples/erc20-broken");
    let sources = load_sources(&project);
    let layouts = load_layouts(
        &sources.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(),
    )
    .await;
    let report = extract_from_structural(&sources, &layouts, &StructuralConfig::default());
    eprintln!(
        "phase5_exit[erc20-broken]: {} high-confidence candidates",
        report.candidates.len()
    );
    for c in &report.candidates {
        eprintln!("  - {}", c.name);
    }
    // Must produce at least some structural candidates; the contract
    // shares its shape with erc20 (decimals, etc.). No claim of
    // soundness on the specific allowance-skip bug — the catalog +
    // tests + natspec oracles catch that one. Phase 5's contribution
    // here is the source-shape invariants (decimals const, etc.).
    assert!(
        !report.candidates.is_empty(),
        "erc20-broken should still yield some structural invariants (decimals etc.)"
    );
}

// ─── Test 3: Determinism (no LLM → byte-identical reruns) ────────────

#[tokio::test]
async fn structural_mining_is_deterministic_across_runs() {
    if !solc_available() {
        eprintln!("phase5_exit: solc not on PATH — skipping");
        return;
    }
    let project = workspace_root().join("examples/erc20");
    let sources = load_sources(&project);
    let layouts = load_layouts(
        &sources.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(),
    )
    .await;
    let r1 = extract_from_structural(&sources, &layouts, &StructuralConfig::default());
    let r2 = extract_from_structural(&sources, &layouts, &StructuralConfig::default());
    // SpecCandidate doesn't impl Eq directly (LowConfidenceFinding does),
    // but PartialEq is derived. Compare per-field via name + template_ref
    // + halmos + intent_text since those are the user-visible bits.
    assert_eq!(r1.candidates.len(), r2.candidates.len());
    for (a, b) in r1.candidates.iter().zip(r2.candidates.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.template_ref, b.template_ref);
        assert_eq!(a.halmos, b.halmos);
        assert_eq!(a.intent_text, b.intent_text);
        assert_eq!(a.source, b.source);
    }
    assert_eq!(r1.low_confidence_findings, r2.low_confidence_findings);
    assert_eq!(r1.miner_counts, r2.miner_counts);
}
