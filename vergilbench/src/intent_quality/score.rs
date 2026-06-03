//! Per-contract scorer for the intent-quality overlay.
//!
//! Given a hand-written ground-truth intent and the multi-oracle's
//! proposed intents, computes:
//!   - **recall**: did any proposal share the ground-truth's taxon?
//!   - **matching_source**: which Stage-1 oracle drove the first
//!     matching proposal?
//!   - **precision**: of N proposals, how many fall into a non-Other
//!     taxon?
//!   - **lexical_jaccard_max**: max Jaccard token-set similarity
//!     between the ground-truth text and any proposal text.
//!
//! Slice 5's aggregate reporter folds these per-contract scores into
//! per-taxon and per-source breakdowns for the final report.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use vergil_core::synthesis::Source;

use super::ground_truth::BenchGroundTruth;
use super::proposed::ProposedIntent;
use super::taxon::{classify, IntentTaxon};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractIntentScore {
    pub contract: String,
    pub ground_truth_taxon: IntentTaxon,
    pub proposed_taxons: Vec<IntentTaxon>,
    pub recall: bool,
    /// First proposal whose taxon matches the ground-truth's taxon.
    /// `None` when `recall == false` or `proposed` is empty.
    pub matching_source: Option<Source>,
    /// Fraction of proposals in a non-Other taxon. `0.0` when
    /// `proposed` is empty.
    pub precision: f32,
    /// Max Jaccard token-set similarity between the ground-truth text
    /// and any proposal text. `0.0` when `proposed` is empty.
    pub lexical_jaccard_max: f32,
}

/// Score one contract. Pure function; no I/O.
pub fn contract(
    name: &str,
    gt: &BenchGroundTruth,
    proposed: &[ProposedIntent],
) -> ContractIntentScore {
    let gt_taxon = classify(&gt.intent_text);
    let gt_tokens = tokenize(&gt.intent_text);

    let proposed_taxons: Vec<IntentTaxon> =
        proposed.iter().map(|p| classify(&p.intent_text)).collect();

    let mut matching_source = None;
    let mut recall = false;
    for (i, t) in proposed_taxons.iter().enumerate() {
        if *t == gt_taxon {
            recall = true;
            matching_source = Some(proposed[i].source);
            break;
        }
    }

    let precision = if proposed.is_empty() {
        0.0
    } else {
        let non_other = proposed_taxons
            .iter()
            .filter(|t| **t != IntentTaxon::Other)
            .count();
        non_other as f32 / proposed.len() as f32
    };

    let lexical_jaccard_max = proposed
        .iter()
        .map(|p| jaccard(&gt_tokens, &tokenize(&p.intent_text)))
        .fold(0.0_f32, f32::max);

    ContractIntentScore {
        contract: name.to_string(),
        ground_truth_taxon: gt_taxon,
        proposed_taxons,
        recall,
        matching_source,
        precision,
        lexical_jaccard_max,
    }
}

/// Small stop-word list. Trimmed deliberately: keeping enough content
/// words to make Jaccard meaningful, removing only the most common
/// glue words that dilute every comparison.
const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "of", "is", "and", "to", "in", "on", "for", "verify", "that", "are", "be",
    "as", "by", "or", "with", "it", "its",
];

fn tokenize(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty() && !STOP_WORDS.contains(t))
        .map(String::from)
        .collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    let inter = a.intersection(b).count();
    inter as f32 / union as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gt(intent: &str) -> BenchGroundTruth {
        BenchGroundTruth {
            intent_text: intent.to_string(),
            property_names: vec![],
            provenance: None,
        }
    }

    fn propose(intent: &str, source: Source) -> ProposedIntent {
        ProposedIntent {
            id: format!("{source:?}:test"),
            source,
            intent_text: intent.to_string(),
            rationale: String::new(),
            confidence: 0.8,
            template_ref: None,
        }
    }

    #[test]
    fn recall_with_match_and_source_attribution() {
        let g = gt("transfers conserve balances and total supply");
        let p = vec![
            propose("the contract reports the underlying token", Source::NatSpec),
            propose(
                "transfer preserves total supply across all calls",
                Source::Tests,
            ),
        ];
        let s = contract("erc20", &g, &p);
        assert_eq!(s.ground_truth_taxon, IntentTaxon::BalanceConservation);
        assert!(s.recall);
        assert_eq!(s.matching_source, Some(Source::Tests));
        assert!(s.precision > 0.5);
    }

    #[test]
    fn no_recall_when_taxons_dont_overlap() {
        let g = gt("decimals returns 6");
        let p = vec![propose(
            "transfers conserve balances and total supply",
            Source::Tests,
        )];
        let s = contract("erc20-decimals", &g, &p);
        assert_eq!(
            s.ground_truth_taxon,
            IntentTaxon::ConfigurationAndIntrospection
        );
        assert!(!s.recall);
        assert!(s.matching_source.is_none());
    }

    #[test]
    fn empty_proposed_yields_zero_recall_zero_precision() {
        let g = gt("only the owner may mutate");
        let s = contract("ownable", &g, &[]);
        assert_eq!(s.ground_truth_taxon, IntentTaxon::AccessPolicy);
        assert!(!s.recall);
        assert!(s.matching_source.is_none());
        assert_eq!(s.precision, 0.0);
        assert_eq!(s.lexical_jaccard_max, 0.0);
        assert!(s.proposed_taxons.is_empty());
    }

    #[test]
    fn precision_excludes_other_bucket() {
        let g = gt("transfers conserve balances");
        let p = vec![
            // SupplyConservation, not Other
            propose("supply by exactly the minted amount", Source::Tests),
            // Other (no keyword fires)
            propose("xxxx yyyy zzzz nothing meaningful", Source::NatSpec),
        ];
        let s = contract("erc20", &g, &p);
        assert_eq!(s.precision, 0.5);
    }

    #[test]
    fn first_match_wins_for_source_attribution() {
        // Two proposals both classify as the ground-truth's taxon; the
        // first one's source should win.
        let g = gt("transfers conserve balances and total supply");
        let p = vec![
            propose("transfers conserve balances", Source::AttackCatalog),
            propose("conserve balances differently", Source::Tests),
        ];
        let s = contract("erc20", &g, &p);
        assert!(s.recall);
        assert_eq!(s.matching_source, Some(Source::AttackCatalog));
    }

    #[test]
    fn lexical_jaccard_picks_best_overlap() {
        let g = gt("transfers conserve balances and total supply");
        let p = vec![
            propose("decimals returns six", Source::NatSpec),
            propose(
                "transfers conserve balances and supply totally",
                Source::Tests,
            ),
        ];
        let s = contract("erc20", &g, &p);
        assert!(
            s.lexical_jaccard_max > 0.4,
            "expected high overlap, got {}",
            s.lexical_jaccard_max
        );
    }

    #[test]
    fn tokenizer_filters_stop_words_and_lowercases() {
        let t = tokenize("Verify ERC-20 conformance: transfers conserve balances");
        assert!(!t.contains("verify"));
        assert!(!t.contains("the"));
        assert!(t.contains("transfers"));
        assert!(t.contains("balances"));
        assert!(t.contains("erc"));
        assert!(t.contains("20"));
    }
}
