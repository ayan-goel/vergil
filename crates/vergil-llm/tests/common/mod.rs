//! Test helpers shared across vergil-llm integration tests.
//!
//! `init_live_env` exists so `--features llm-live` tests can read `.env`
//! at the repo root without forcing every contributor to `export` keys
//! per shell. Production code never depends on dotenvy — it reads env
//! vars directly so the binary works in any environment.

use std::path::PathBuf;
use std::sync::Once;

static INIT: Once = Once::new();

/// Load `<repo>/.env` into the process environment exactly once per test
/// binary. Idempotent and safe to call from every `#[tokio::test]` that
/// needs live API keys.
pub fn init_live_env() {
    INIT.call_once(|| {
        // Walk up from CARGO_MANIFEST_DIR (crates/vergil-llm) to the repo
        // root and load .env if present. dotenvy::dotenv() searches CWD by
        // default which is fragile under `cargo test`.
        let manifest = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR is always set under cargo test");
        let repo_root = PathBuf::from(manifest)
            .parent()
            .and_then(|p| p.parent())
            .expect("crates/vergil-llm has a grandparent (repo root)")
            .to_path_buf();
        let _ = dotenvy::from_path(repo_root.join(".env"));
    });
}
