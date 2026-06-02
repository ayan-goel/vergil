//! Vergil CLI library exports.
//!
//! Binaries in this crate (the `vergil` command + the `kill-criterion`
//! runner) share the same intent pipeline via this library. The
//! `#[path]` directive keeps the source in `commands/intent.rs` so the
//! main binary's `mod commands;` still works.
//!
//! V1.5 Phase 6 Slice 4 surfaces the `output` module so the lib-side
//! `intent` module can reach `crate::output::layout` for the
//! tier-aware `vergil-out/` paths. Both the binary's `main.rs` and the
//! library's `lib.rs` declare `mod output;` against the same on-disk
//! `output/mod.rs`; they compile into two distinct module copies but
//! resolve to the same source.

pub mod output;

#[path = "commands/intent.rs"]
pub mod intent;
