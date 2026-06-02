//! VergilBench library crate.
//!
//! Most pipeline logic lives in the binaries (`runner`, `calibration`). This
//! library hosts cross-binary modules — currently the intent-quality overlay
//! invoked from the runner when `--intent-quality` is set after a
//! `--zero-config` sweep.

pub mod intent_quality;
