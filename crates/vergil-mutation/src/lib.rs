//! Gambit mutation testing wrapper. Generates mutants of a Solidity file
//! and scores a spec by counting how many mutants the spec catches.
//!
//! Pipeline (per SPEC §3.7):
//!   1. [`Mutator::generate`] runs `gambit mutate --filename <sol> --outdir <tmp>`
//!      and parses `gambit_results.json` into a typed [`Mutant`] list.
//!   2. [`MutationScorer`] applies each mutant by overwriting the source
//!      file with the mutant's `.sol`, re-runs Halmos against the spec
//!      via a caller-supplied `MutationRunner`, and tallies kills.
//!   3. Coverage = killed / total. SPEC §3.7 drops specs below 0.4.
//!
//! Degraded mode: if `gambit` is not on PATH, [`Mutator::generate`] returns
//! [`MutationError::GambitMissing`] and the loop should treat every spec
//! as coverage = 1.0 with a `mutation_unavailable` flag set in the report.

pub mod gambit;
pub mod scorer;

pub use gambit::{Mutant, MutationError, Mutator};
pub use scorer::{MutationRunner, MutationScore, MutationScorer, MutationVerdict, ScoreError};
