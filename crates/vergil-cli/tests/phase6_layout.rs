//! V1.5 Phase 6 Slice 4 — `vergil-out/` tier-aware layout integration test.
//!
//! Phase 6 SPEC §3.8 defines the artifact tree shape. Slice 4 added a
//! central `output::layout` path-builder and routed the Phase-1 +
//! intent writers through it (Slice 8 introduces the full per-tier
//! writes via the unified runner). This integration test exercises
//! `ensure_tree` from the lib API surface and asserts every directory
//! the SPEC names exists.
//!
//! A regression here means a Phase-6-consumer (Slice 5's verdict
//! formatter, Slice 6's cex sink, Slice 7's confirmation gate) would
//! land on a path that doesn't exist.

use std::path::PathBuf;

use vergil_cli::output::layout::{
    attack_catalog_per_template_proof, confirm_state, counterexamples_dir, ensure_tree, report_md,
    smt_dir, source_dir, tier_dir, top_level_proof_json, trace_jsonl, vergil_out,
};
use vergil_proof::schema::{Source, Tier};

#[test]
fn ensure_tree_lays_out_spec_3_8_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    ensure_tree(project).expect("ensure_tree");

    // Top-level
    assert!(vergil_out(project).is_dir(), "vergil-out/ root must exist");
    assert!(
        counterexamples_dir(project).is_dir(),
        "vergil-out/counterexamples/ must exist"
    );
    assert!(smt_dir(project).is_dir(), "vergil-out/smt/ must exist");

    // Zero-config tier subdirs (every Phase 6 source).
    for s in [
        Source::AttackCatalog,
        Source::Conformance,
        Source::Tests,
        Source::NatSpec,
        Source::Structural,
    ] {
        let dir = source_dir(project, Tier::ZeroConfig, s);
        assert!(
            dir.is_dir(),
            "zero-config source dir missing: {} (for {:?})",
            dir.display(),
            s
        );
    }

    // Intent tier dir.
    assert!(tier_dir(project, Tier::Intent).is_dir());
}

#[test]
fn layout_helper_paths_match_spec_3_8_exactly() {
    // Surface-level lock on the path strings — a rename of any of these
    // is a breaking change for downstream consumers (Slice 5's
    // verdict formatter writes report.md / Slice 8's runner writes
    // per-template proofs / Slice 7 writes confirm/state.json).
    let project = PathBuf::from("/p");
    assert_eq!(
        top_level_proof_json(&project),
        PathBuf::from("/p/vergil-out/proof.json")
    );
    assert_eq!(
        report_md(&project),
        PathBuf::from("/p/vergil-out/report.md")
    );
    assert_eq!(
        counterexamples_dir(&project),
        PathBuf::from("/p/vergil-out/counterexamples")
    );
    assert_eq!(smt_dir(&project), PathBuf::from("/p/vergil-out/smt"));
    assert_eq!(
        trace_jsonl(&project),
        PathBuf::from("/p/vergil-out/trace/run.jsonl")
    );
    assert_eq!(
        confirm_state(&project),
        PathBuf::from("/p/vergil-out/confirm/state.json")
    );
    assert_eq!(
        attack_catalog_per_template_proof(&project, "reentrancy-cei"),
        PathBuf::from("/p/vergil-out/zero-config/attack-catalog/reentrancy-cei.proof.json")
    );
}
