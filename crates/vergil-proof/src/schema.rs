//! Serde types for `proof.json` — schema_version = 1 (frozen for Phase 2).

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
}
