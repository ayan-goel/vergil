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
//!   1. Load the hand-written intent from `properties.yaml` (this module).
//!   2. Load the multi-oracle proposed intents from
//!      `vergil-out/confirm/state.json`.
//!   3. Classify both into a 9-bucket taxonomy.
//!   4. Score per-contract recall + per-source attribution.
//!
//! Aggregated, the overlay reports per-taxon recall and which Stage-1
//! oracle (catalog / tests / natspec / structural) drove each match.
//!
//! Zero LLM cost — pure structural comparison on artifacts the sweep
//! already wrote. Builds in 8 slices per
//! `tasks/v1.5-intent-quality-plan.md`.

use std::path::{Path, PathBuf};

pub mod ground_truth;
pub mod proposed;
pub mod report;
pub mod score;
pub mod taxon;

/// Slice 6 wires the real overlay. S0-S5 ship a stub so the runner's
/// `--intent-quality` flag has a callable target without behavior change.
pub fn run_overlay(
    _corpus: &Path,
    _contracts: &[PathBuf],
    _sweep_result: &Path,
) -> Result<(), String> {
    eprintln!("[vergilbench] intent-quality overlay: stub (Slices 2-6 not yet shipped)");
    Ok(())
}
