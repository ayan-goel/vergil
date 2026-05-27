//! Vergil CLI library exports.
//!
//! Binaries in this crate (the `vergil` command + the `kill-criterion`
//! runner) share the same intent pipeline via this library. The
//! `#[path]` directive keeps the source in `commands/intent.rs` so the
//! main binary's `mod commands;` still works.

#[path = "commands/intent.rs"]
pub mod intent;
