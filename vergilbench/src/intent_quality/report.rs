//! Aggregate reporter for the intent-quality overlay.
//!
//! Folds a `Vec<ContractIntentScore>` into a single
//! `AggregateIntentReport` with:
//!   - per-taxon recall (which property flavors are well-covered)
//!   - per-source contribution (which Stage-1 oracle drives matches)
//!   - precision histogram (how noisy are the proposals)
//!   - low-recall + zero-proposed watchlists (where to look first)
//!
//! Also renders a human-readable markdown summary the runner writes to
//! `intent-quality-summary.md` next to the timestamped sweep JSON.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vergil_core::synthesis::Source;

use super::score::ContractIntentScore;
use super::taxon::IntentTaxon;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregateIntentReport {
    pub per_taxon: BTreeMap<IntentTaxon, TaxonStats>,
    pub source_contribution: BTreeMap<String, usize>,
    /// Buckets `[0, 0.1), [0.1, 0.2), ..., [0.9, 1.0]`. The last bucket
    /// includes 1.0 exactly.
    pub precision_histogram: [usize; 10],
    pub low_recall_contracts: Vec<String>,
    pub zero_proposed_contracts: Vec<String>,
    pub total_contracts: usize,
    pub total_recalled: usize,
    pub overall_recall_rate: f32,
    pub mean_jaccard: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TaxonStats {
    pub total: usize,
    pub recalled: usize,
    pub rate: f32,
}

pub fn aggregate(scores: &[ContractIntentScore]) -> AggregateIntentReport {
    let total_contracts = scores.len();

    let mut per_taxon: BTreeMap<IntentTaxon, TaxonStats> = BTreeMap::new();
    for score in scores {
        let entry = per_taxon.entry(score.ground_truth_taxon).or_default();
        entry.total += 1;
        if score.recall {
            entry.recalled += 1;
        }
    }
    for stats in per_taxon.values_mut() {
        stats.rate = if stats.total > 0 {
            stats.recalled as f32 / stats.total as f32
        } else {
            0.0
        };
    }

    let mut source_contribution: BTreeMap<String, usize> = BTreeMap::new();
    for score in scores {
        if let Some(src) = score.matching_source {
            *source_contribution.entry(source_label(src)).or_insert(0) += 1;
        }
    }

    let mut precision_histogram = [0_usize; 10];
    for score in scores {
        let p = score.precision.clamp(0.0, 1.0);
        let bin = ((p * 10.0) as usize).min(9);
        precision_histogram[bin] += 1;
    }

    let mut low_recall_contracts: Vec<String> = scores
        .iter()
        .filter(|s| !s.recall && !s.proposed_taxons.is_empty())
        .map(|s| s.contract.clone())
        .collect();
    low_recall_contracts.sort();

    let mut zero_proposed_contracts: Vec<String> = scores
        .iter()
        .filter(|s| s.proposed_taxons.is_empty())
        .map(|s| s.contract.clone())
        .collect();
    zero_proposed_contracts.sort();

    let total_recalled = scores.iter().filter(|s| s.recall).count();
    let overall_recall_rate = if total_contracts > 0 {
        total_recalled as f32 / total_contracts as f32
    } else {
        0.0
    };

    let mean_jaccard = if total_contracts > 0 {
        scores.iter().map(|s| s.lexical_jaccard_max).sum::<f32>() / total_contracts as f32
    } else {
        0.0
    };

    AggregateIntentReport {
        per_taxon,
        source_contribution,
        precision_histogram,
        low_recall_contracts,
        zero_proposed_contracts,
        total_contracts,
        total_recalled,
        overall_recall_rate,
        mean_jaccard,
    }
}

impl AggregateIntentReport {
    pub fn render_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# Intent-quality overlay\n\n");
        md.push_str(&format!(
            "**Overall recall**: {}/{} ({:.1}%)  \n",
            self.total_recalled,
            self.total_contracts,
            self.overall_recall_rate * 100.0,
        ));
        md.push_str(&format!(
            "**Mean lexical Jaccard (max-per-contract)**: {:.2}\n\n",
            self.mean_jaccard,
        ));

        md.push_str("## Per-taxon recall\n\n");
        md.push_str("| Taxon | Recalled | Total | Rate |\n");
        md.push_str("|---|---|---|---|\n");
        for (taxon, stats) in &self.per_taxon {
            md.push_str(&format!(
                "| `{taxon}` | {} | {} | {:.1}% |\n",
                stats.recalled,
                stats.total,
                stats.rate * 100.0,
            ));
        }

        md.push_str("\n## Source contribution (which oracle drove the match)\n\n");
        md.push_str("| Source | Matches |\n");
        md.push_str("|---|---|\n");
        for (src, n) in &self.source_contribution {
            md.push_str(&format!("| `{src}` | {n} |\n"));
        }

        md.push_str("\n## Precision histogram\n\n");
        md.push_str("Fraction of proposals classified into a non-Other taxon.\n\n");
        md.push_str("| Bucket | Contracts |\n");
        md.push_str("|---|---|\n");
        for (i, count) in self.precision_histogram.iter().enumerate() {
            let lo = i as f32 / 10.0;
            let hi = (i + 1) as f32 / 10.0;
            md.push_str(&format!("| [{lo:.1}, {hi:.1}) | {count} |\n"));
        }

        if !self.zero_proposed_contracts.is_empty() {
            md.push_str("\n## Contracts with zero proposed intents\n\n");
            md.push_str("Typically utility libraries: the catalog activates nothing and there are no tests / natspec.\n\n");
            for c in &self.zero_proposed_contracts {
                md.push_str(&format!("- `{c}`\n"));
            }
        }

        if !self.low_recall_contracts.is_empty() {
            md.push_str("\n## Low-recall contracts (proposals exist, none match the ground-truth taxon)\n\n");
            md.push_str("Worth manual review — the proposed intents may be correct-but-tangential, or the ground-truth intent may be unusual.\n\n");
            for c in &self.low_recall_contracts {
                md.push_str(&format!("- `{c}`\n"));
            }
        }

        md
    }
}

/// Stable string id per Source variant. Used as the BTreeMap key in
/// `source_contribution` so JSON output is human-readable.
fn source_label(s: Source) -> String {
    match s {
        Source::AttackCatalog => "catalog".to_string(),
        Source::Tests => "tests".to_string(),
        Source::NatSpec => "natspec".to_string(),
        Source::Structural => "structural".to_string(),
        Source::UserIntent => "user-intent".to_string(),
        Source::Conformance => "conformance".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(
        contract: &str,
        gt_taxon: IntentTaxon,
        proposed: &[IntentTaxon],
        recall: bool,
        matching_source: Option<Source>,
        precision: f32,
        jaccard: f32,
    ) -> ContractIntentScore {
        ContractIntentScore {
            contract: contract.to_string(),
            ground_truth_taxon: gt_taxon,
            proposed_taxons: proposed.to_vec(),
            recall,
            matching_source,
            precision,
            lexical_jaccard_max: jaccard,
        }
    }

    #[test]
    fn empty_input_yields_zero_totals() {
        let agg = aggregate(&[]);
        assert_eq!(agg.total_contracts, 0);
        assert_eq!(agg.total_recalled, 0);
        assert_eq!(agg.overall_recall_rate, 0.0);
        assert_eq!(agg.mean_jaccard, 0.0);
        assert!(agg.per_taxon.is_empty());
        assert!(agg.source_contribution.is_empty());
    }

    #[test]
    fn per_taxon_rate_is_recalled_over_total() {
        let scores = vec![
            score(
                "c1",
                IntentTaxon::BalanceConservation,
                &[IntentTaxon::BalanceConservation],
                true,
                Some(Source::Tests),
                1.0,
                0.5,
            ),
            score(
                "c2",
                IntentTaxon::BalanceConservation,
                &[IntentTaxon::Other],
                false,
                None,
                0.0,
                0.1,
            ),
            score(
                "c3",
                IntentTaxon::AccessPolicy,
                &[IntentTaxon::AccessPolicy, IntentTaxon::Other],
                true,
                Some(Source::AttackCatalog),
                0.5,
                0.3,
            ),
        ];
        let agg = aggregate(&scores);

        assert_eq!(agg.total_contracts, 3);
        assert_eq!(agg.total_recalled, 2);
        assert!((agg.overall_recall_rate - 0.6667).abs() < 0.001);

        let bc = agg
            .per_taxon
            .get(&IntentTaxon::BalanceConservation)
            .unwrap();
        assert_eq!(bc.total, 2);
        assert_eq!(bc.recalled, 1);
        assert_eq!(bc.rate, 0.5);

        let ap = agg.per_taxon.get(&IntentTaxon::AccessPolicy).unwrap();
        assert_eq!(ap.recalled, 1);
        assert_eq!(ap.total, 1);
        assert_eq!(ap.rate, 1.0);
    }

    #[test]
    fn source_contribution_counts_first_match_only() {
        let scores = vec![
            score(
                "c1",
                IntentTaxon::BalanceConservation,
                &[IntentTaxon::BalanceConservation],
                true,
                Some(Source::Tests),
                1.0,
                0.5,
            ),
            score(
                "c2",
                IntentTaxon::AccessPolicy,
                &[IntentTaxon::AccessPolicy],
                true,
                Some(Source::AttackCatalog),
                1.0,
                0.5,
            ),
            score(
                "c3",
                IntentTaxon::Reentrancy,
                &[IntentTaxon::Reentrancy],
                true,
                Some(Source::Tests),
                1.0,
                0.5,
            ),
        ];
        let agg = aggregate(&scores);
        assert_eq!(agg.source_contribution.get("tests"), Some(&2));
        assert_eq!(agg.source_contribution.get("catalog"), Some(&1));
    }

    #[test]
    fn low_recall_excludes_zero_proposed() {
        let scores = vec![
            score(
                "has_proposals_no_recall",
                IntentTaxon::AccessPolicy,
                &[IntentTaxon::BalanceConservation],
                false,
                None,
                1.0,
                0.0,
            ),
            score(
                "zero_proposals",
                IntentTaxon::AccessPolicy,
                &[],
                false,
                None,
                0.0,
                0.0,
            ),
            score(
                "recalled",
                IntentTaxon::AccessPolicy,
                &[IntentTaxon::AccessPolicy],
                true,
                Some(Source::Tests),
                1.0,
                1.0,
            ),
        ];
        let agg = aggregate(&scores);
        assert_eq!(agg.low_recall_contracts, vec!["has_proposals_no_recall"]);
        assert_eq!(agg.zero_proposed_contracts, vec!["zero_proposals"]);
    }

    #[test]
    fn precision_histogram_bins_into_tenths() {
        let scores = vec![
            score("a", IntentTaxon::AccessPolicy, &[], false, None, 0.0, 0.0),
            score(
                "b",
                IntentTaxon::AccessPolicy,
                &[IntentTaxon::AccessPolicy],
                true,
                Some(Source::Tests),
                0.5,
                0.5,
            ),
            score(
                "c",
                IntentTaxon::AccessPolicy,
                &[IntentTaxon::AccessPolicy],
                true,
                Some(Source::Tests),
                1.0,
                1.0,
            ),
        ];
        let agg = aggregate(&scores);
        assert_eq!(agg.precision_histogram[0], 1); // 0.0
        assert_eq!(agg.precision_histogram[5], 1); // 0.5
        assert_eq!(agg.precision_histogram[9], 1); // 1.0 (clamped to last bin)
    }

    #[test]
    fn markdown_render_includes_all_sections() {
        let scores = vec![
            score(
                "c1",
                IntentTaxon::BalanceConservation,
                &[IntentTaxon::BalanceConservation],
                true,
                Some(Source::Tests),
                1.0,
                0.5,
            ),
            score(
                "noisy",
                IntentTaxon::AccessPolicy,
                &[IntentTaxon::Other, IntentTaxon::Other],
                false,
                None,
                0.0,
                0.0,
            ),
            score("empty", IntentTaxon::Reentrancy, &[], false, None, 0.0, 0.0),
        ];
        let md = aggregate(&scores).render_markdown();
        assert!(md.contains("# Intent-quality overlay"));
        assert!(md.contains("Overall recall**: 1/3"));
        assert!(md.contains("## Per-taxon recall"));
        assert!(md.contains("balance-conservation"));
        assert!(md.contains("## Source contribution"));
        assert!(md.contains("| `tests` | 1 |"));
        assert!(md.contains("## Precision histogram"));
        assert!(md.contains("## Contracts with zero proposed intents"));
        assert!(md.contains("- `empty`"));
        assert!(md.contains("## Low-recall contracts"));
        assert!(md.contains("- `noisy`"));
    }
}
