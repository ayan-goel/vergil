//! Integration test for the intent-quality overlay.
//!
//! Loads 5 committed fixtures under
//! `tests/fixtures/intent_quality/`, materializes them into a tempdir
//! mimicking the runtime layout (each contract's
//! `state-fixture.json` becomes `vergil-out/confirm/state.json`), runs
//! the overlay through the crate's public API, and asserts the
//! aggregated report shape.
//!
//! Fixture inventory:
//!   - cons-recalled        : Conservation gt + matching proposal (tests)
//!   - access-recalled      : AccessPolicy gt + matching proposal (catalog)
//!   - structural-recalled  : Monotonicity gt + matching proposal (structural)
//!   - mismatched-taxon     : Configuration gt + Conservation proposal (no recall)
//!   - zero-proposed-utility-lib : utility-lib shape (no state.json)
//!
//! Expected aggregate:
//!   3/5 recalled, 1 mismatched, 1 zero-proposed,
//!   matching sources: tests=1, catalog=1, structural=1.

use std::fs;
use std::path::PathBuf;

use vergilbench::intent_quality::{report::AggregateIntentReport, run_overlay};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/intent_quality")
}

fn materialize_corpus(target: &std::path::Path) {
    let contracts_dir = target.join("contracts");
    fs::create_dir_all(&contracts_dir).unwrap();

    for entry in fs::read_dir(fixtures_root()).unwrap().flatten() {
        let src_dir = entry.path();
        if !src_dir.is_dir() {
            continue;
        }
        let name = src_dir.file_name().unwrap().to_str().unwrap().to_string();
        let dst_dir = contracts_dir.join(&name);
        fs::create_dir_all(&dst_dir).unwrap();

        // Always copy properties.yaml
        fs::copy(
            src_dir.join("properties.yaml"),
            dst_dir.join("properties.yaml"),
        )
        .unwrap();

        // If state-fixture.json exists, place it at vergil-out/confirm/state.json
        let state_src = src_dir.join("state-fixture.json");
        if state_src.is_file() {
            let confirm_dir = dst_dir.join("vergil-out").join("confirm");
            fs::create_dir_all(&confirm_dir).unwrap();
            fs::copy(state_src, confirm_dir.join("state.json")).unwrap();
        }
    }
}

#[test]
fn fixture_corpus_produces_expected_overlay() {
    let td = tempfile::TempDir::new().unwrap();
    let corpus = td.path();
    materialize_corpus(corpus);

    // Build contracts vec in deterministic alphabetical order (matches
    // the bench runner's `entries.sort()` invariant).
    let contracts_dir = corpus.join("contracts");
    let mut contracts: Vec<PathBuf> = fs::read_dir(&contracts_dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    contracts.sort();

    let results_dir = corpus.join("results");
    fs::create_dir_all(&results_dir).unwrap();
    let sweep_result = results_dir.join("fixture-sweep.json");
    fs::write(&sweep_result, "{}").unwrap();

    run_overlay(corpus, &contracts, &sweep_result).expect("overlay completes");

    let json_out = results_dir.join("fixture-sweep.intent-quality.json");
    let md_out = results_dir.join("fixture-sweep.intent-quality.md");
    assert!(json_out.is_file());
    assert!(md_out.is_file());

    let agg: AggregateIntentReport =
        serde_json::from_str(&fs::read_to_string(&json_out).unwrap()).unwrap();

    assert_eq!(agg.total_contracts, 5, "all 5 fixtures counted");
    assert_eq!(
        agg.total_recalled, 3,
        "3 recalled: cons, access, structural"
    );
    assert!(
        (agg.overall_recall_rate - 0.6).abs() < 0.01,
        "60% recall, got {}",
        agg.overall_recall_rate
    );

    assert_eq!(
        agg.zero_proposed_contracts,
        vec!["zero-proposed-utility-lib"],
        "utility-lib classified as zero-proposed",
    );
    assert_eq!(
        agg.low_recall_contracts,
        vec!["mismatched-taxon"],
        "only the mismatched one has proposals-but-no-recall",
    );

    // Source attribution: one per source.
    assert_eq!(agg.source_contribution.get("tests"), Some(&1));
    assert_eq!(agg.source_contribution.get("catalog"), Some(&1));
    assert_eq!(agg.source_contribution.get("structural"), Some(&1));
    assert!(
        !agg.source_contribution.contains_key("natspec"),
        "no natspec recall in this fixture set",
    );

    // Markdown rendering carries the headline + sections.
    let md = fs::read_to_string(&md_out).unwrap();
    assert!(md.contains("Overall recall**: 3/5"));
    assert!(md.contains("- `zero-proposed-utility-lib`"));
    assert!(md.contains("- `mismatched-taxon`"));
}
