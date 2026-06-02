//! Structural mining — V1.5 Phase 5.
//!
//! Fourth Stage-1 oracle alongside `catalog_intent`, `tests_intent`,
//! `natspec_intent`. Mines candidate properties **deterministically**
//! from solc storage layout + regex over Solidity function bodies. Zero
//! LLM cost. Phase 6 deferred Phase 5 but left stable seams:
//!
//! - `Source::Structural` already in the enum at `synthesis.rs`.
//! - `output::layout::source_dir(ZeroConfig, Structural)` already wired.
//! - `critique::source_guidance(Source::Structural)` already defaults
//!   the `restate_the_source` axis to 1.0 (structural candidates aren't
//!   paraphrased text).
//!
//! Five miner families per SPEC §3.5:
//!
//! 1. Invariant constants (state vars assigned only at declaration or
//!    in the constructor)
//! 2. Monotonicity (state vars only ever incremented or only ever
//!    decremented)
//! 3. Access policy (every public write to slot s requires modifier m)
//! 4. Conservation (paired `M[a] -= k; M[b] += k` preserves a sum)
//! 5. Two-step patterns (F2 requires gate var A which only F1 writes)
//!
//! Slice 0 ships the data plumbing + an empty `extract_from_structural`
//! stub. Slices 1-5 add each miner.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use vergil_solidity::storage::StorageLayout;

use crate::synthesis::{Source, SpecCandidate};

/// Identifier for one of the five Phase 5 miner families. Used in
/// telemetry counters and to encode confidence into a candidate's
/// `template_ref` (`"structural:{id}:{conf:.2}"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StructuralMiner {
    InvariantConstants,
    Monotonicity,
    AccessPolicy,
    Conservation,
    TwoStep,
}

impl StructuralMiner {
    /// Stable kebab-case identifier. Pinned by V2 billing + verdict UI.
    pub fn id(self) -> &'static str {
        match self {
            Self::InvariantConstants => "invariant-constants",
            Self::Monotonicity => "monotonicity",
            Self::AccessPolicy => "access-policy",
            Self::Conservation => "conservation",
            Self::TwoStep => "two-step",
        }
    }

    /// Iterate all five miners in stable declaration order.
    pub fn all() -> [StructuralMiner; 5] {
        [
            Self::InvariantConstants,
            Self::Monotonicity,
            Self::AccessPolicy,
            Self::Conservation,
            Self::TwoStep,
        ]
    }
}

/// One mined candidate before the confidence cut. The pipeline sees
/// only the inner [`SpecCandidate`] (via [`Self::into_spec_candidate`]);
/// the confidence + miner survive only in the report's
/// `low_confidence_findings` for below-threshold candidates.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralCandidate {
    pub spec: SpecCandidate,
    /// In `[0.0, 1.0]`. Candidates with `confidence >= cfg.min_confidence`
    /// enter the verification pipeline; the rest stay in the report.
    pub confidence: f32,
    pub miner: StructuralMiner,
}

impl StructuralCandidate {
    /// Consume into a bare [`SpecCandidate`], encoding the confidence
    /// into `template_ref` as `"structural:{miner}:{conf:.2}"` so it
    /// survives the synthesis → critique → SMT pipeline and surfaces in
    /// the verdict / `proof.json` artifact. Mutates `source` to
    /// [`Source::Structural`] and never overwrites an existing
    /// `template_ref` (the miner controls the format).
    pub fn into_spec_candidate(self) -> SpecCandidate {
        let mut spec = self.spec;
        spec.source = Source::Structural;
        let template_ref = spec.template_ref.unwrap_or_else(|| {
            format!("structural:{}:{:.2}", self.miner.id(), self.confidence)
        });
        spec.template_ref = Some(template_ref);
        spec
    }
}

/// Below-threshold finding — surfaced in the verdict's "Suggested
/// additional invariants" section but NOT submitted to the solver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LowConfidenceFinding {
    pub miner: StructuralMiner,
    pub description: String,
    /// Stored as a string to keep the report's serde shape simple and
    /// avoid float-equality test brittleness. Format: `"{conf:.2}"`.
    pub confidence: String,
    pub fn_or_var: Option<String>,
}

impl LowConfidenceFinding {
    pub fn new(miner: StructuralMiner, description: impl Into<String>, confidence: f32) -> Self {
        Self {
            miner,
            description: description.into(),
            confidence: format!("{confidence:.2}"),
            fn_or_var: None,
        }
    }

    pub fn with_target(mut self, fn_or_var: impl Into<String>) -> Self {
        self.fn_or_var = Some(fn_or_var.into());
        self
    }
}

/// Aggregated output of one structural-mining pass.
#[derive(Debug, Default, Clone)]
pub struct StructuralReport {
    /// Confidence ≥ `cfg.min_confidence`. These flow into Stage 1's
    /// merged candidate list.
    pub candidates: Vec<SpecCandidate>,
    /// Below-threshold; report-only.
    pub low_confidence_findings: Vec<LowConfidenceFinding>,
    /// Per-miner count of emitted high-confidence candidates. Stable
    /// keying via [`StructuralMiner::id`] for telemetry.
    pub miner_counts: HashMap<StructuralMiner, usize>,
}

/// Configuration for one structural-mining run.
#[derive(Debug, Clone)]
pub struct StructuralConfig {
    /// Cutoff below which candidates go to `low_confidence_findings`
    /// instead of `candidates`. Default 0.6 per SPEC §11.5.
    pub min_confidence: f32,
}

impl Default for StructuralConfig {
    fn default() -> Self {
        Self { min_confidence: 0.6 }
    }
}

/// Phase 5 oracle entry point. Sync + no LLM dependency — Phase 5 is
/// pure static analysis (solc storage layout + regex over function
/// bodies). Slice 0 ships an empty stub; Slices 1-5 add each miner.
///
/// `sources` is a list of `(path, source_text)` pairs — Phase 5 mines
/// across every Solidity source the fingerprint identified.
/// `layouts` is the per-contract solc storage layout (one entry per
/// `<file>:<ContractName>`), produced by
/// `vergil_solidity::storage::StorageRun`.
pub fn extract_from_structural(
    _sources: &[(PathBuf, String)],
    _layouts: &[StorageLayout],
    _cfg: &StructuralConfig,
) -> StructuralReport {
    // Slice 0 — empty oracle. Verdict stays unchanged when this
    // returns; Slices 1-5 populate.
    StructuralReport::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_spec(name: &str) -> SpecCandidate {
        SpecCandidate {
            name: name.into(),
            halmos: format!(
                "function {name}() public {{ assert(true); }}"
            ),
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: false,
            source: Source::UserIntent,
            intent_text: None,
        }
    }

    #[test]
    fn miner_ids_are_stable_kebab_case() {
        assert_eq!(StructuralMiner::InvariantConstants.id(), "invariant-constants");
        assert_eq!(StructuralMiner::Monotonicity.id(), "monotonicity");
        assert_eq!(StructuralMiner::AccessPolicy.id(), "access-policy");
        assert_eq!(StructuralMiner::Conservation.id(), "conservation");
        assert_eq!(StructuralMiner::TwoStep.id(), "two-step");
    }

    #[test]
    fn miner_all_returns_five_in_stable_order() {
        let v = StructuralMiner::all();
        assert_eq!(v.len(), 5);
        assert_eq!(v[0], StructuralMiner::InvariantConstants);
        assert_eq!(v[4], StructuralMiner::TwoStep);
    }

    #[test]
    fn into_spec_candidate_tags_source_and_encodes_confidence() {
        let sc = StructuralCandidate {
            spec: dummy_spec("check_owner_const"),
            confidence: 0.95,
            miner: StructuralMiner::InvariantConstants,
        };
        let out = sc.into_spec_candidate();
        assert_eq!(out.source, Source::Structural);
        assert_eq!(
            out.template_ref.as_deref(),
            Some("structural:invariant-constants:0.95")
        );
    }

    #[test]
    fn into_spec_candidate_preserves_explicit_template_ref() {
        // A miner that already filled in a richer template_ref should
        // NOT be clobbered by the default format.
        let mut spec = dummy_spec("check_x");
        spec.template_ref = Some("structural:custom".into());
        let sc = StructuralCandidate {
            spec,
            confidence: 0.7,
            miner: StructuralMiner::Conservation,
        };
        let out = sc.into_spec_candidate();
        assert_eq!(out.template_ref.as_deref(), Some("structural:custom"));
    }

    #[test]
    fn empty_extract_returns_default_report() {
        let cfg = StructuralConfig::default();
        let r = extract_from_structural(&[], &[], &cfg);
        assert!(r.candidates.is_empty());
        assert!(r.low_confidence_findings.is_empty());
        assert!(r.miner_counts.is_empty());
    }

    #[test]
    fn config_default_threshold_is_06() {
        let cfg = StructuralConfig::default();
        assert!((cfg.min_confidence - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn low_confidence_finding_format() {
        let f = LowConfidenceFinding::new(
            StructuralMiner::TwoStep,
            "F2 requires F1",
            0.55,
        )
        .with_target("commit_reveal");
        assert_eq!(f.confidence, "0.55");
        assert_eq!(f.fn_or_var.as_deref(), Some("commit_reveal"));
    }
}
