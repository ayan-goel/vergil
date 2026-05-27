//! Proof artifact schema (`proof.json`) and re-verifier (`vergil prove`).
//!
//! The artifact captures everything a third party with the same toolchain
//! needs to reproduce a verification: source SHAs, toolchain versions, the
//! verified-property list with backend + wall-clock, counterexamples,
//! quality metrics, and run cost. Schema version is `1` and frozen for
//! Phase 2 per SPEC §10.2 — breaking changes wait for a coordinated bump.

pub mod schema;
pub mod verify;

pub use schema::{
    Cost, CounterexampleSummary, ProofArtifact, QualityMetrics, RunMeta, SourceFile,
    ToolchainVersions, VerifiedProperty,
};
pub use verify::{verify_artifact, ProveError, ProveReport};
