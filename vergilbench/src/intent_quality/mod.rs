//! Intent-quality overlay for zero-config sweeps.
//!
//! Phase 7's headline kill criterion (SPEC §11.7) measures verification
//! rate: of N contracts run in zero-config mode, how many pass their
//! applicable catalog subset. Because the bench harness auto-confirms via
//! `--yes`, that metric answers "do our oracles propose intents the
//! verifier can prove?" — but not "are the intents we propose the ones a
//! developer would have hand-written?".
//!
//! This module fills that gap. For each contract:
//!   1. Load the hand-written intent from `properties.yaml`.
//!   2. Load the multi-oracle proposed intents from
//!      `vergil-out/confirm/state.json`.
//!   3. Classify both into a 10+Other taxonomy.
//!   4. Score per-contract recall + per-source attribution.
//!
//! Aggregated, the overlay reports per-taxon recall and which Stage-1
//! oracle (catalog / tests / natspec / structural) drove each match.
//!
//! Zero LLM cost — pure structural comparison on artifacts the sweep
//! already wrote. Built across 8 slices per
//! `tasks/v1.5-intent-quality-plan.md`.

use std::path::{Path, PathBuf};

pub mod ground_truth;
pub mod proposed;
pub mod report;
pub mod score;
pub mod taxon;

/// Post-sweep entry point invoked by the runner when `--intent-quality`
/// is set. Walks each contract, scores it, writes JSON + markdown
/// reports next to `sweep_result`.
pub fn run_overlay(
    corpus: &Path,
    contracts: &[PathBuf],
    sweep_result: &Path,
) -> Result<(), String> {
    let gt_map = ground_truth::load(corpus)?;

    let mut scores: Vec<score::ContractIntentScore> = Vec::new();
    for contract_path in contracts {
        let name = match contract_path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => {
                eprintln!(
                    "[intent-quality] skip {}: invalid path",
                    contract_path.display()
                );
                continue;
            }
        };
        let Some(gt) = gt_map.get(name) else {
            eprintln!("[intent-quality] skip {name}: no ground truth in corpus");
            continue;
        };
        let proposed = proposed::load(contract_path)?;
        scores.push(score::contract(name, gt, &proposed));
    }

    let agg = report::aggregate(&scores);

    // Write outputs alongside the sweep result. Filenames are derived
    // from the sweep's stem so each timestamped sweep gets a paired
    // overlay artifact.
    let dir = sweep_result
        .parent()
        .ok_or_else(|| format!("sweep result has no parent dir: {}", sweep_result.display()))?;
    let stem = sweep_result
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("sweep");

    let json_out = dir.join(format!("{stem}.intent-quality.json"));
    let md_out = dir.join(format!("{stem}.intent-quality.md"));

    let json_body =
        serde_json::to_string_pretty(&agg).map_err(|e| format!("serialize intent-quality: {e}"))?;
    std::fs::write(&json_out, json_body)
        .map_err(|e| format!("write {}: {e}", json_out.display()))?;
    eprintln!("[vergilbench] wrote {}", json_out.display());

    let md_body = agg.render_markdown();
    std::fs::write(&md_out, md_body).map_err(|e| format!("write {}: {e}", md_out.display()))?;
    eprintln!("[vergilbench] wrote {}", md_out.display());

    eprintln!(
        "[vergilbench] intent-quality: recall {}/{} ({:.1}%)",
        agg.total_recalled,
        agg.total_contracts,
        agg.overall_recall_rate * 100.0,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::fs;
    use tempfile::TempDir;
    use vergil_core::confirm::{ConfirmState, ProposedIntent};
    use vergil_core::synthesis::Source;

    /// Smoke test: build a tempdir corpus with 3 contracts in 3 distinct
    /// shapes (recalled, missed, zero-proposed), run the overlay,
    /// inspect the JSON + markdown outputs.
    #[test]
    fn run_overlay_end_to_end_on_tempdir_corpus() {
        let td = TempDir::new().unwrap();
        let corpus = td.path();
        let contracts_dir = corpus.join("contracts");

        // Contract 1: ground-truth conservation + proposal that also classifies as conservation
        let c1 = contracts_dir.join("recalled-erc20");
        fs::create_dir_all(&c1).unwrap();
        fs::write(
            c1.join("properties.yaml"),
            r#"
version: 1
intent: "transfers conserve balances and total supply"
properties: []
"#,
        )
        .unwrap();
        write_state(
            &c1,
            vec![ProposedIntent {
                id: "tests:check_supply".into(),
                source: Source::Tests,
                intent_text: "transfer preserves total supply across all calls".into(),
                rationale: "".into(),
                confidence: 0.85,
                template_ref: None,
            }],
        );

        // Contract 2: ground-truth access-policy + proposal that classifies as something else (no recall)
        let c2 = contracts_dir.join("missed-ownable");
        fs::create_dir_all(&c2).unwrap();
        fs::write(
            c2.join("properties.yaml"),
            r#"
version: 1
intent: "only the owner may mutate the counter"
properties: []
"#,
        )
        .unwrap();
        write_state(
            &c2,
            vec![ProposedIntent {
                id: "natspec:something".into(),
                source: Source::NatSpec,
                intent_text: "decimals returns 18".into(),
                rationale: "".into(),
                confidence: 0.7,
                template_ref: None,
            }],
        );

        // Contract 3: ground-truth reentrancy + no vergil-out (utility-lib shape)
        let c3 = contracts_dir.join("zero-proposed-libcontract");
        fs::create_dir_all(&c3).unwrap();
        fs::write(
            c3.join("properties.yaml"),
            r#"
version: 1
intent: "a normal (non-reentrant) call to a guarded function succeeds"
properties: []
"#,
        )
        .unwrap();
        // No vergil-out/

        // Set up the sweep result file
        let results_dir = corpus.join("results");
        fs::create_dir_all(&results_dir).unwrap();
        let sweep_result = results_dir.join("2026-06-02-test.json");
        fs::write(&sweep_result, "{}").unwrap();

        let contracts = vec![c1.clone(), c2.clone(), c3.clone()];
        run_overlay(corpus, &contracts, &sweep_result).expect("overlay runs cleanly");

        // Inspect outputs
        let json_out = results_dir.join("2026-06-02-test.intent-quality.json");
        let md_out = results_dir.join("2026-06-02-test.intent-quality.md");
        assert!(json_out.is_file(), "json output written");
        assert!(md_out.is_file(), "markdown output written");

        let body = fs::read_to_string(&json_out).unwrap();
        let agg: report::AggregateIntentReport = serde_json::from_str(&body).unwrap();
        assert_eq!(agg.total_contracts, 3);
        assert_eq!(agg.total_recalled, 1);
        assert_eq!(
            agg.zero_proposed_contracts,
            vec!["zero-proposed-libcontract"]
        );
        assert_eq!(agg.low_recall_contracts, vec!["missed-ownable"]);
        assert_eq!(agg.source_contribution.get("tests"), Some(&1));

        let md = fs::read_to_string(&md_out).unwrap();
        assert!(md.contains("Overall recall**: 1/3"));
        assert!(md.contains("- `zero-proposed-libcontract`"));
        assert!(md.contains("- `missed-ownable`"));
    }

    #[test]
    fn skips_contracts_without_ground_truth() {
        let td = TempDir::new().unwrap();
        let corpus = td.path();
        fs::create_dir_all(corpus.join("contracts/known")).unwrap();
        fs::write(
            corpus.join("contracts/known/properties.yaml"),
            "version: 1\nintent: \"transfers conserve balances\"\nproperties: []",
        )
        .unwrap();

        let stranger = TempDir::new().unwrap();
        let stranger_path = stranger.path().join("not-in-corpus");
        fs::create_dir_all(&stranger_path).unwrap();

        let results_dir = corpus.join("results");
        fs::create_dir_all(&results_dir).unwrap();
        let sweep_result = results_dir.join("s.json");
        fs::write(&sweep_result, "{}").unwrap();

        let contracts = vec![corpus.join("contracts/known"), stranger_path];
        run_overlay(corpus, &contracts, &sweep_result).expect("overlay runs cleanly");

        let json_out = results_dir.join("s.intent-quality.json");
        let body = fs::read_to_string(&json_out).unwrap();
        let agg: report::AggregateIntentReport = serde_json::from_str(&body).unwrap();
        assert_eq!(
            agg.total_contracts, 1,
            "stranger path skipped, only known counted"
        );
    }

    fn write_state(contract_dir: &Path, intents: Vec<ProposedIntent>) {
        let dir = contract_dir.join("vergil-out").join("confirm");
        fs::create_dir_all(&dir).unwrap();
        let state = ConfirmState::new("rid".into(), intents, Utc::now());
        fs::write(
            dir.join("state.json"),
            serde_json::to_string_pretty(&state).unwrap(),
        )
        .unwrap();
    }
}
