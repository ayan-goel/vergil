//! Serde types for `proof.json` — schema_version = 1 (frozen for Phase 2).
//!
//! V1.5 Phase 6 Slice 2 extends [`VerifiedProperty`] with two
//! provenance fields (`tier` + `source`) needed by the stratified
//! verdict (SPEC §3.6). Both fields carry `#[serde(default)]` so V1
//! artifacts deserialize unchanged: missing `tier` → `Tier::Intent`,
//! missing `source` → `Source::UserIntent`. The schema_version stays
//! at 1 — the additions are backward-compatible, not a breaking change.
//!
//! The [`Source`] / [`Tier`] enums are mirrored locally rather than
//! pulled from `vergil_core::synthesis::Source` to keep this crate
//! lightweight (vergil-proof is the artifact-read boundary; external
//! consumers shouldn't need the full async stack). Wire-compatible
//! JSON: same `rename_all = "snake_case"` shape as
//! `vergil_core::synthesis::Source`. `vergil-cli` does the conversion
//! at the construction boundary.

use serde::{Deserialize, Serialize};

/// Top-level proof artifact written to `vergil-out/proof.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProofArtifact {
    pub vergil_version: String,
    pub schema_version: u32,
    pub run: RunMeta,
    pub toolchain: ToolchainVersions,
    pub source_files: Vec<SourceFile>,
    pub verified_properties: Vec<VerifiedProperty>,
    #[serde(default)]
    pub counterexamples: Vec<CounterexampleSummary>,
    pub quality_metrics: QualityMetrics,
    pub cost: Cost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunMeta {
    pub run_id: String,
    pub intent: String,
    pub project_root: String,
    /// ISO-8601 UTC timestamp of when the run started.
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolchainVersions {
    pub solc: String,
    pub halmos: String,
    pub slither: String,
    pub z3: String,
    pub cvc5: String,
    #[serde(default)]
    pub gambit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifiedProperty {
    pub name: String,
    pub backend: String,
    /// SHA-256 of the Halmos check_ function source.
    pub spec_sha256: String,
    /// Optional template id this property was derived from (synthesizer hint).
    #[serde(default)]
    pub template_ref: Option<String>,
    pub wall_clock_ms: u64,
    /// SHA-256 of the SMT-LIB query Halmos / SMTChecker dispatched. When
    /// the backend doesn't expose the query directly, this is null and
    /// `vergil prove` re-runs the backend to re-derive the verdict.
    #[serde(default)]
    pub smt_query_sha256: Option<String>,
    pub manifest_validation: ManifestValidationStatus,
    /// Tier the property landed in: zero-config (any Stage-1 oracle —
    /// catalog, tests, NatSpec, structural, conformance) or
    /// intent (V1 CEGIS over a user-typed `--intent` or
    /// `properties.yaml`). V1 artifacts default to `Intent` so old
    /// proofs deserialize with V1-correct semantics. Phase 6 SPEC §3.6.
    #[serde(default)]
    pub tier: Tier,
    /// Origin of the candidate property — which Stage-1 oracle (if any)
    /// proposed the underlying intent. V1 artifacts default to
    /// `UserIntent`. The stratified verdict (SPEC §3.6) groups proven
    /// properties by source. Phase 6 SPEC §3.6.
    #[serde(default)]
    pub source: Source,
}

/// Which Phase-6 tier a verified property landed in. V1 single-tier
/// proofs default to `Intent` so existing artifacts re-verify with
/// V1-correct semantics. SPEC §3.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    /// Stage 1 auto-coverage: catalog activation, tests-derived,
    /// NatSpec-derived, structural-mined, or interface-conformance.
    /// No user-typed intent.
    ZeroConfig,
    /// V1 CEGIS path over a user-typed `--intent` or
    /// `properties.yaml` row.
    #[default]
    Intent,
}

/// Origin of a verified property's candidate. Mirrors
/// `vergil_core::synthesis::Source` with the same `snake_case` JSON
/// wire format so artifacts cross the crate boundary unchanged. V1
/// artifacts (no `source` field) deserialize to `UserIntent` via
/// `#[serde(default)]` on the parent struct. SPEC §3.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// V1 path: candidate came from a user-typed `--intent` or
    /// `properties.yaml` row. Default for V1 artifacts that predate
    /// Phase 6's provenance tagging.
    #[default]
    UserIntent,
    /// Phase 1/2 attack catalog: candidate's intent_text is the
    /// template's `negation_property` field, fed through V1 SYNTHESIZE.
    AttackCatalog,
    /// V1 interface-conformance catalog (the 100-template ERC-X set).
    Conformance,
    /// Phase 4: candidate derived from a test assertion.
    Tests,
    /// Phase 4: candidate derived from a NatSpec doc-comment block.
    NatSpec,
    /// Phase 5: candidate derived from structural mining
    /// (conservation / monotonicity / access-policy / invariant
    /// constants / two-step patterns). Phase 6 reserves the variant;
    /// Phase 5 emits SpecCandidates of this source.
    Structural,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestValidationStatus {
    pub storage_ok: bool,
    pub modifiers_ok: bool,
    pub external_calls_ok: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterexampleSummary {
    pub property: String,
    pub backend: String,
    pub cex_file: String,
    pub wall_clock_ms: u64,
    pub trace_summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// Lowest mutation coverage across all verified properties.
    /// `None` when mutation testing was unavailable (degraded mode).
    pub mutation_coverage_min: Option<f64>,
    /// Fraction of synthesized candidates the critique pass accepted.
    pub critique_pass_rate: f32,
    /// Whether mutation testing ran (false in degraded mode).
    pub mutation_testing_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub usd_estimate: f64,
    pub wall_clock_ms: u64,
}

impl ProofArtifact {
    pub fn schema_version_current() -> u32 {
        1
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != Self::schema_version_current() {
            return Err(format!(
                "schema_version mismatch: got {}, expected {}",
                self.schema_version,
                Self::schema_version_current()
            ));
        }
        if self.source_files.is_empty() {
            return Err("source_files must not be empty".into());
        }
        for f in &self.source_files {
            if f.sha256.len() != 64 {
                return Err(format!(
                    "source_files[{}].sha256 must be 64 hex chars",
                    f.path
                ));
            }
        }
        Ok(())
    }
}

/// Compute the SHA-256 of file contents and return as 64-char lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ProofArtifact {
        ProofArtifact {
            vergil_version: "0.0.1".into(),
            schema_version: 1,
            run: RunMeta {
                run_id: "run-1".into(),
                intent: "preserve totalSupply".into(),
                project_root: "/tmp/p".into(),
                started_at: "2026-05-26T19:30:00Z".into(),
            },
            toolchain: ToolchainVersions {
                solc: "0.8.20".into(),
                halmos: "0.3.3".into(),
                slither: "0.11.0".into(),
                z3: "4.15.4".into(),
                cvc5: "1.3.0".into(),
                gambit: Some("0.2.1".into()),
            },
            source_files: vec![SourceFile {
                path: "src/Token.sol".into(),
                sha256: "a".repeat(64),
            }],
            verified_properties: vec![VerifiedProperty {
                name: "check_x".into(),
                backend: "halmos".into(),
                spec_sha256: "b".repeat(64),
                template_ref: Some("erc20-x".into()),
                wall_clock_ms: 1234,
                smt_query_sha256: None,
                manifest_validation: ManifestValidationStatus {
                    storage_ok: true,
                    modifiers_ok: true,
                    external_calls_ok: true,
                    warnings: Vec::new(),
                },
                tier: Tier::Intent,
                source: Source::UserIntent,
            }],
            counterexamples: Vec::new(),
            quality_metrics: QualityMetrics {
                mutation_coverage_min: Some(0.6),
                critique_pass_rate: 0.8,
                mutation_testing_enabled: true,
            },
            cost: Cost {
                tokens_in: 10_000,
                tokens_out: 4_000,
                usd_estimate: 0.45,
                wall_clock_ms: 50_000,
            },
        }
    }

    #[test]
    fn round_trips_through_json() {
        let a = sample();
        let s = serde_json::to_string_pretty(&a).unwrap();
        let back: ProofArtifact = serde_json::from_str(&s).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn validate_accepts_well_formed() {
        sample().validate().expect("valid");
    }

    #[test]
    fn validate_rejects_wrong_schema_version() {
        let mut a = sample();
        a.schema_version = 99;
        let err = a.validate().unwrap_err();
        assert!(err.contains("schema_version"));
    }

    #[test]
    fn validate_rejects_short_sha() {
        let mut a = sample();
        a.source_files[0].sha256 = "abc".into();
        let err = a.validate().unwrap_err();
        assert!(err.contains("sha256"));
    }

    #[test]
    fn sha256_hex_round_trips() {
        let s = sha256_hex(b"hello vergil");
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
        // Stable: same input → same hash.
        assert_eq!(sha256_hex(b"hello vergil"), s);
    }

    // ─── Phase 6 Slice 2: tier + source provenance ───────────────────────

    /// V1 artifacts predate the `tier` / `source` fields. They must
    /// deserialize cleanly with V1-correct defaults (`tier: intent`,
    /// `source: user_intent`).
    #[test]
    fn v1_proof_json_without_tier_or_source_deserializes_with_defaults() {
        let json = r#"{
            "name": "check_balance_conservation",
            "backend": "halmos",
            "spec_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "template_ref": null,
            "wall_clock_ms": 1500,
            "smt_query_sha256": null,
            "manifest_validation": {
                "storage_ok": true,
                "modifiers_ok": true,
                "external_calls_ok": true,
                "warnings": []
            }
        }"#;
        let prop: VerifiedProperty = serde_json::from_str(json).expect("V1 round-trip");
        assert_eq!(prop.tier, Tier::Intent, "V1 default tier must be Intent");
        assert_eq!(
            prop.source,
            Source::UserIntent,
            "V1 default source must be UserIntent"
        );
    }

    /// Phase 6 artifacts carry explicit provenance. Round-trip must
    /// preserve both fields verbatim (wire format = snake_case).
    #[test]
    fn phase6_proof_json_with_tier_and_source_round_trips() {
        let json = r#"{
            "name": "check_transferFrom_rejects_unauthorized",
            "backend": "halmos",
            "spec_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "template_ref": "access-missing-modifier-state-change",
            "wall_clock_ms": 2200,
            "smt_query_sha256": null,
            "manifest_validation": {
                "storage_ok": true,
                "modifiers_ok": true,
                "external_calls_ok": true,
                "warnings": []
            },
            "tier": "zero-config",
            "source": "attack_catalog"
        }"#;
        let prop: VerifiedProperty = serde_json::from_str(json).expect("Phase 6 round-trip");
        assert_eq!(prop.tier, Tier::ZeroConfig);
        assert_eq!(prop.source, Source::AttackCatalog);
        // Re-serialize and confirm wire shape is preserved.
        let s = serde_json::to_string(&prop).expect("serialize");
        assert!(s.contains("\"tier\":\"zero-config\""), "{s}");
        assert!(s.contains("\"source\":\"attack_catalog\""), "{s}");
    }

    /// The full enum surface must serialize / deserialize for every
    /// variant — this catches a future enum-renaming-without-rename
    /// regression.
    #[test]
    fn source_enum_round_trips_every_variant() {
        let cases = [
            (Source::UserIntent, "\"user_intent\""),
            (Source::AttackCatalog, "\"attack_catalog\""),
            (Source::Conformance, "\"conformance\""),
            (Source::Tests, "\"tests\""),
            (Source::NatSpec, "\"nat_spec\""),
            (Source::Structural, "\"structural\""),
        ];
        for (variant, wire) in cases {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, wire, "Source::{variant:?} should serialize as {wire}");
            let back: Source = serde_json::from_str(&s).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn tier_enum_round_trips_every_variant() {
        let cases = [(Tier::ZeroConfig, "\"zero-config\""), (Tier::Intent, "\"intent\"")];
        for (variant, wire) in cases {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, wire, "Tier::{variant:?} should serialize as {wire}");
            let back: Tier = serde_json::from_str(&s).unwrap();
            assert_eq!(back, variant);
        }
    }
}
