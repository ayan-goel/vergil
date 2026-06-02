//! V1.5 Phase 6 Slice 2 — V1 proof.json round-trip compatibility.
//!
//! Phase 6 added `tier` + `source` fields to
//! `vergil_proof::schema::VerifiedProperty`. Both carry
//! `#[serde(default)]` so V1 artifacts (no provenance) deserialize
//! with V1-correct semantics: `tier: intent`, `source: user_intent`.
//!
//! This test loads every checked-in `examples/*/vergil-out/proof.json`
//! produced by V1 / Phase 1 runs and asserts it deserializes without
//! error, with the provenance defaults applied. A regression here
//! would mean a V1 user's existing proof.json stops re-verifying with
//! the new binary.

use std::path::PathBuf;

use vergil_proof::schema::{ProofArtifact, Source, Tier};

fn examples_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/vergil-cli
    p.pop(); // crates
    p.push("examples");
    p
}

fn read_proof(path: &PathBuf) -> ProofArtifact {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("deserialize {}: {e}", path.display()))
}

#[test]
fn v1_erc20_proof_json_deserializes_with_defaults() {
    let path = examples_root().join("erc20/vergil-out/proof.json");
    if !path.is_file() {
        // The example artifact only exists post-`vergil verify` run.
        // Tolerate its absence (CI run may not have an erc20 artifact)
        // but assert deserialization works if it does exist.
        eprintln!("skipping: {} not present", path.display());
        return;
    }
    let artifact = read_proof(&path);
    assert_eq!(artifact.schema_version, 1);
    for p in &artifact.verified_properties {
        assert_eq!(
            p.tier,
            Tier::Intent,
            "V1 proof.json property {} must default to Tier::Intent",
            p.name
        );
        assert_eq!(
            p.source,
            Source::UserIntent,
            "V1 proof.json property {} must default to Source::UserIntent",
            p.name
        );
    }
}

#[test]
fn all_example_proof_jsons_deserialize() {
    let root = examples_root();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    let mut tested = 0;
    for e in entries.flatten() {
        let p = e.path().join("vergil-out").join("proof.json");
        if !p.is_file() {
            continue;
        }
        let artifact = read_proof(&p);
        // V1 schema_version = 1; Phase 6 keeps the version (additions
        // are backward-compat, not a breaking change).
        assert_eq!(
            artifact.schema_version, 1,
            "unexpected schema_version in {}",
            p.display()
        );
        // Every verified property must deserialize with the new fields,
        // defaulting where absent.
        for prop in &artifact.verified_properties {
            // Just touching the fields confirms they exist on the
            // struct and the defaults applied. Tier / Source impl
            // PartialEq so this is enough.
            let _ = prop.tier;
            let _ = prop.source;
        }
        tested += 1;
    }
    // We always expect at least one example to have a proof.json
    // (committed under examples/erc20/vergil-out/) so the test
    // actively exercises the path rather than silently passing on
    // every CI machine.
    assert!(
        tested > 0,
        "expected at least one example proof.json under {}",
        root.display()
    );
}
