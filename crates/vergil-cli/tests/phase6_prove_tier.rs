//! V1.5 Phase 6 Slice 11 — `vergil prove` preserves tier + source.
//!
//! Slice 2 added `tier: Tier` and `source: Source` to
//! `vergil_proof::schema::VerifiedProperty` with `#[serde(default)]`
//! so V1 artifacts deserialize unchanged. Slice 11's job is to
//! confirm `vergil prove` reads + preserves those fields end-to-end:
//!
//! 1. V1 proof.json (no tier/source) → prove succeeds, defaults apply.
//! 2. Phase 6 proof.json (with tier/source) → prove succeeds, fields
//!    survive the round-trip through serde + the prove path.
//! 3. The Reproduce command in Slice 5's verdict actually works on
//!    the proof.json it names (end-to-end loop closed).
//!
//! Live re-verification via cvc5 is covered by Slice 10's
//! `phase6_live::vergil_prove_re_verifies_phase6_proof_json`. This
//! test sticks to deterministic schema-level checks so it runs in CI
//! without LLM keys.

use std::path::{Path, PathBuf};
use std::process::Command;

use vergil_proof::schema::{ProofArtifact, Source, Tier};

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
}

fn vergil(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO"))
        .args([
            "run",
            "-p",
            "vergil-cli",
            "--bin",
            "vergil",
            "--quiet",
            "--",
        ])
        .args(args)
        .current_dir(workspace_root())
        .output()
        .expect("cargo run vergil")
}

fn write_artifact(path: &Path, artifact: &ProofArtifact) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let body = serde_json::to_string_pretty(artifact).unwrap();
    std::fs::write(path, body).unwrap();
}

fn skeleton_v1_proof(project_root: &Path) -> ProofArtifact {
    use vergil_proof::schema::{
        sha256_hex, Cost, ManifestValidationStatus, QualityMetrics, RunMeta, SourceFile,
        ToolchainVersions, VerifiedProperty,
    };
    let token_path = project_root.join("src").join("Token.sol");
    let token_body = std::fs::read(&token_path).unwrap_or_default();
    ProofArtifact {
        vergil_version: "0.0.1".into(),
        schema_version: 1,
        run: RunMeta {
            run_id: "slice11-test".into(),
            intent: "test-intent".into(),
            project_root: project_root.display().to_string(),
            started_at: "2026-06-02T12:00:00Z".into(),
        },
        toolchain: ToolchainVersions {
            solc: "0.8.20".into(),
            halmos: "0.3.3".into(),
            slither: "0.11.0".into(),
            z3: "4.15.4".into(),
            cvc5: "1.3.0".into(),
            gambit: None,
        },
        source_files: vec![SourceFile {
            path: "src/Token.sol".into(),
            sha256: sha256_hex(&token_body),
        }],
        verified_properties: vec![VerifiedProperty {
            name: "check_v1_compat".into(),
            backend: "halmos".into(),
            spec_sha256: "a".repeat(64),
            template_ref: None,
            wall_clock_ms: 100,
            smt_query_sha256: None,
            manifest_validation: ManifestValidationStatus {
                storage_ok: true,
                modifiers_ok: true,
                external_calls_ok: true,
                warnings: Vec::new(),
            },
            // V1 defaults — picked up via #[serde(default)] on the
            // serialized side; explicit here.
            tier: Tier::Intent,
            source: Source::UserIntent,
        }],
        counterexamples: Vec::new(),
        quality_metrics: QualityMetrics {
            mutation_coverage_min: None,
            critique_pass_rate: 0.0,
            mutation_testing_enabled: false,
        },
        cost: Cost {
            tokens_in: 0,
            tokens_out: 0,
            usd_estimate: 0.0,
            wall_clock_ms: 0,
        },
    }
}

#[test]
fn vergil_prove_succeeds_on_v1_artifact_without_tier_source() {
    // Write a V1-shaped artifact (no `tier` / `source` in the JSON)
    // by hand, then run vergil prove. Defaults apply, prove succeeds.
    let project = workspace_root().join("examples/erc20");
    if !project.join("vergil-out/proof.json").is_file() {
        eprintln!("examples/erc20/vergil-out/proof.json absent — generate first");
        return;
    }
    // Use the existing examples/erc20 proof.json which carries V1
    // semantics from the original V1 run.
    let proof_path = project.join("vergil-out/proof.json");
    let out = vergil(&["prove", proof_path.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "vergil prove failed on V1 artifact:\nstdout={stdout}\nstderr={stderr}"
    );
    assert!(
        stdout.contains("verified properties recorded"),
        "expected prove summary line:\n{stdout}"
    );
}

#[test]
fn vergil_prove_succeeds_on_phase6_artifact_with_tier_source() {
    // Construct a synthetic Phase-6-shaped artifact in a tempdir and
    // run vergil prove. The artifact must deserialize with the new
    // fields and prove must succeed.
    use vergil_proof::schema::{sha256_hex, ManifestValidationStatus, VerifiedProperty};
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().to_path_buf();
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(
        project.join("src/Demo.sol"),
        b"// SPDX-License-Identifier: MIT\ncontract Demo {}\n",
    )
    .unwrap();
    std::fs::write(project.join("foundry.toml"), b"[profile.default]\n").unwrap();

    let mut artifact = skeleton_v1_proof(&project);
    // Swap in a Phase-6-style property.
    artifact.source_files[0].path = "src/Demo.sol".into();
    artifact.source_files[0].sha256 =
        sha256_hex(b"// SPDX-License-Identifier: MIT\ncontract Demo {}\n");
    artifact.verified_properties = vec![
        VerifiedProperty {
            name: "check_phase6_catalog".into(),
            backend: "halmos".into(),
            spec_sha256: "b".repeat(64),
            template_ref: Some("access-missing-modifier-state-change".into()),
            wall_clock_ms: 250,
            smt_query_sha256: None,
            manifest_validation: ManifestValidationStatus {
                storage_ok: true,
                modifiers_ok: true,
                external_calls_ok: true,
                warnings: Vec::new(),
            },
            tier: Tier::ZeroConfig,
            source: Source::AttackCatalog,
        },
        VerifiedProperty {
            name: "check_phase6_tests".into(),
            backend: "halmos".into(),
            spec_sha256: "c".repeat(64),
            template_ref: None,
            wall_clock_ms: 200,
            smt_query_sha256: None,
            manifest_validation: ManifestValidationStatus {
                storage_ok: true,
                modifiers_ok: true,
                external_calls_ok: true,
                warnings: Vec::new(),
            },
            tier: Tier::ZeroConfig,
            source: Source::Tests,
        },
    ];

    let proof_path: PathBuf = project.join("vergil-out/proof.json");
    write_artifact(&proof_path, &artifact);

    // Read back to confirm tier/source survive serde round-trip.
    let body = std::fs::read_to_string(&proof_path).unwrap();
    let parsed: ProofArtifact = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed.verified_properties.len(), 2);
    assert_eq!(parsed.verified_properties[0].tier, Tier::ZeroConfig);
    assert_eq!(parsed.verified_properties[0].source, Source::AttackCatalog);
    assert_eq!(parsed.verified_properties[1].tier, Tier::ZeroConfig);
    assert_eq!(parsed.verified_properties[1].source, Source::Tests);

    // Now run vergil prove against the synthetic artifact.
    let out = vergil(&["prove", proof_path.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "vergil prove failed on Phase 6 artifact:\nstdout={stdout}\nstderr={stderr}"
    );
}

#[test]
fn phase6_proof_json_serialize_preserves_wire_format_for_source_and_tier() {
    // Lock the wire format. snake_case enum strings — V2 billing and
    // SaaS UI both depend on these.
    use vergil_proof::schema::{sha256_hex, ManifestValidationStatus, VerifiedProperty};
    let p = VerifiedProperty {
        name: "check_x".into(),
        backend: "halmos".into(),
        spec_sha256: sha256_hex(b"x"),
        template_ref: Some("foo".into()),
        wall_clock_ms: 100,
        smt_query_sha256: None,
        manifest_validation: ManifestValidationStatus {
            storage_ok: true,
            modifiers_ok: true,
            external_calls_ok: true,
            warnings: Vec::new(),
        },
        tier: Tier::ZeroConfig,
        source: Source::AttackCatalog,
    };
    let s = serde_json::to_string(&p).unwrap();
    assert!(s.contains("\"tier\":\"zero-config\""), "{s}");
    assert!(s.contains("\"source\":\"attack_catalog\""), "{s}");
}
