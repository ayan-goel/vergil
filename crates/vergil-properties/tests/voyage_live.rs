//! Live integration test (--features llm-live): exercise the real Voyage
//! API once to confirm the embedder + retriever stack works end-to-end
//! against a non-mock provider.

#![cfg(feature = "llm-live")]

use std::path::PathBuf;
use std::sync::Once;

use vergil_properties::{Catalog, Retriever, VoyageEmbedder};

static INIT: Once = Once::new();

fn load_env() {
    INIT.call_once(|| {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo_root = manifest.parent().unwrap().parent().unwrap();
        let _ = dotenvy::from_path(repo_root.join(".env"));
    });
}

#[tokio::test]
async fn voyage_round_trip_against_catalog() {
    load_env();
    let Ok(key) = std::env::var("VOYAGE_API_KEY") else {
        eprintln!("voyage_round_trip: VOYAGE_API_KEY not in env, skipping");
        return;
    };
    let cat = Catalog::load(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates"))
        .expect("catalog");

    let tmp = tempfile::tempdir().unwrap();
    let retriever = Retriever::new(cat, Box::new(VoyageEmbedder::new(key)), tmp.path())
        .await
        .expect("retriever construction succeeds against real Voyage");

    let hits = retriever
        .retrieve(
            "Standard ERC-20: balances sum to totalSupply; transfers respect allowances",
            5,
        )
        .await
        .expect("retrieve succeeds");

    assert_eq!(hits.len(), 5);
    // At least one ERC-20 template should appear in the top-5 for this intent.
    assert!(
        hits.iter().any(|h| h.template_id.starts_with("erc20-")),
        "no ERC-20 templates in top 5: {hits:?}"
    );
    for w in hits.windows(2) {
        assert!(w[0].score >= w[1].score);
    }
}
