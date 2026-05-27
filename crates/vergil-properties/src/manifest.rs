//! Cross-check a [`PropertyManifest`] against authoritative static analysis
//! (solc storage layout + Slither detectors) **before** the property is
//! handed to a solver. SPEC §3.8: a manifest that disagrees with the
//! authoritative source is a structural failure, not a soundness silent-pass.
//!
//! Strict checks (block dispatch):
//!   * Every declared storage slot must exist in the solc layout under the
//!     same `label`, with a type whose label matches the manifest's
//!     `solidity_type` field.
//!
//! Soft checks (warn but accept — Slither's basic detector report doesn't
//! expose per-function modifier reachability or call-graph edges; deeper
//! integration is queued for Phase 3):
//!   * Slither HIGH-impact reentrancy / storage detectors flagged on the
//!     contract while the manifest declares `external_call_handling: havoc`
//!     produce an advisory warning that propagates with the validation result.
//!
//! No validation step ever silently widens or downgrades.

use thiserror::Error;

use vergil_solidity::slither::SlitherReport;
use vergil_solidity::storage::StorageLayout;

use crate::catalog::{PropertyManifest, StorageSlotReq};

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error(
        "{template}: storage slot `{slot}` not present in solc layout (looked across {searched} contracts)"
    )]
    StorageSlotMissing {
        template: String,
        slot: String,
        searched: usize,
    },
    #[error(
        "{template}: storage slot `{slot}` type mismatch: manifest declares `{expected}` but solc reports `{actual}`"
    )]
    StorageSlotTypeMismatch {
        template: String,
        slot: String,
        expected: String,
        actual: String,
    },
    #[error("{template}: solc returned no storage layouts; cannot validate")]
    NoLayouts { template: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    /// Soft warnings (Slither HIGH-impact findings, missing-modifier hints).
    /// The validation passes — these are surfaced for the run report, not
    /// gates. They do NOT shadow strict errors (those return `Err`).
    pub warnings: Vec<String>,
}

/// Cross-check a manifest against the authoritative static analysis.
/// Strict failures return `Err`; soft warnings flow through the `Ok` value.
pub fn validate(
    manifest: &PropertyManifest,
    storage: &[StorageLayout],
    slither: &SlitherReport,
) -> Result<ValidationReport, ManifestError> {
    if storage.is_empty() {
        return Err(ManifestError::NoLayouts {
            template: manifest.id.clone(),
        });
    }
    for req in &manifest.requires.storage_slots {
        check_storage_slot(&manifest.id, req, storage)?;
    }
    let warnings = soft_check_external_calls(manifest, slither);
    Ok(ValidationReport { warnings })
}

fn check_storage_slot(
    template: &str,
    req: &StorageSlotReq,
    layouts: &[StorageLayout],
) -> Result<(), ManifestError> {
    for layout in layouts {
        for entry in &layout.entries {
            if entry.label == req.name {
                let actual = layout
                    .types
                    .get(&entry.type_id)
                    .map(|t| t.label.as_str())
                    .unwrap_or(entry.type_id.as_str());
                if types_match(&req.solidity_type, actual) {
                    return Ok(());
                }
                return Err(ManifestError::StorageSlotTypeMismatch {
                    template: template.to_string(),
                    slot: req.name.clone(),
                    expected: req.solidity_type.clone(),
                    actual: actual.to_string(),
                });
            }
        }
    }
    Err(ManifestError::StorageSlotMissing {
        template: template.to_string(),
        slot: req.name.clone(),
        searched: layouts.len(),
    })
}

/// Loose comparison: solc emits "mapping(address => uint256)" with single
/// spaces; manifests may use the same shape. Normalize whitespace.
fn types_match(expected: &str, actual: &str) -> bool {
    normalize(expected) == normalize(actual)
}

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn soft_check_external_calls(manifest: &PropertyManifest, slither: &SlitherReport) -> Vec<String> {
    let mut out = Vec::new();
    let declares_havoc = manifest
        .requires
        .external_call_handling
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("havoc"))
        .unwrap_or(false);
    if !declares_havoc {
        return out;
    }
    for det in &slither.detectors {
        if det.impact == "High"
            && (det.check.contains("reentrancy")
                || det.check.contains("unchecked-lowlevel")
                || det.check.contains("arbitrary-send"))
        {
            out.push(format!(
                "{}: slither HIGH `{}` may interact with havoc'd external-call assumption",
                manifest.id, det.check
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use vergil_solidity::slither::Detector;
    use vergil_solidity::storage::{StorageEntry, StorageType};

    use super::*;
    use crate::catalog::{AppliesTo, CostClass, EncodingPaths, Provenance, Requires, Tier};

    fn make_manifest(slots: Vec<StorageSlotReq>) -> PropertyManifest {
        PropertyManifest {
            id: "test-template".to_string(),
            description: "test".to_string(),
            cost_class: CostClass::Cheap,
            applies_to: AppliesTo::default(),
            requires: Requires {
                storage_slots: slots,
                modifiers: Vec::new(),
                external_call_handling: Some("havoc".to_string()),
            },
            encoding: EncodingPaths {
                halmos: "halmos.sol".into(),
                smtchecker: None,
            },
            provenance: Provenance {
                tier: Tier::Original,
                source: "test".into(),
                inspired_by: None,
                license: "Apache-2.0".into(),
                upstream_commit: None,
            },
        }
    }

    fn make_layout(entries: Vec<(&str, &str, &str)>) -> StorageLayout {
        let mut types = HashMap::new();
        let mut storage = Vec::new();
        for (label, type_id, type_label) in entries {
            types.insert(
                type_id.to_string(),
                StorageType {
                    label: type_label.to_string(),
                    encoding: "inplace".to_string(),
                    number_of_bytes: "32".to_string(),
                },
            );
            storage.push(StorageEntry {
                label: label.to_string(),
                slot: "0".to_string(),
                offset: 0,
                type_id: type_id.to_string(),
                contract: "TestContract".to_string(),
            });
        }
        StorageLayout {
            qualified_name: "Test.sol:TestContract".to_string(),
            entries: storage,
            types,
        }
    }

    #[test]
    fn passes_when_every_slot_present_with_matching_type() {
        let manifest = make_manifest(vec![StorageSlotReq {
            name: "_balances".into(),
            solidity_type: "mapping(address => uint256)".into(),
        }]);
        let layout = make_layout(vec![(
            "_balances",
            "t_mapping_address_uint256",
            "mapping(address => uint256)",
        )]);
        let slither = SlitherReport { detectors: vec![] };
        let report = validate(&manifest, &[layout], &slither).expect("ok");
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn missing_slot_is_error() {
        let manifest = make_manifest(vec![StorageSlotReq {
            name: "_phantom".into(),
            solidity_type: "uint256".into(),
        }]);
        let layout = make_layout(vec![("_balances", "t_uint256", "uint256")]);
        let slither = SlitherReport { detectors: vec![] };
        let err = validate(&manifest, &[layout], &slither).unwrap_err();
        assert!(
            matches!(err, ManifestError::StorageSlotMissing { .. }),
            "{err}"
        );
    }

    #[test]
    fn type_mismatch_is_error() {
        let manifest = make_manifest(vec![StorageSlotReq {
            name: "_balances".into(),
            solidity_type: "uint128".into(),
        }]);
        let layout = make_layout(vec![("_balances", "t_uint256", "uint256")]);
        let slither = SlitherReport { detectors: vec![] };
        let err = validate(&manifest, &[layout], &slither).unwrap_err();
        match err {
            ManifestError::StorageSlotTypeMismatch {
                expected, actual, ..
            } => {
                assert_eq!(expected, "uint128");
                assert_eq!(actual, "uint256");
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn no_layouts_is_error() {
        let manifest = make_manifest(vec![]);
        let slither = SlitherReport { detectors: vec![] };
        let err = validate(&manifest, &[], &slither).unwrap_err();
        assert!(matches!(err, ManifestError::NoLayouts { .. }));
    }

    #[test]
    fn high_reentrancy_with_havoc_produces_warning() {
        let manifest = make_manifest(vec![]);
        let layout = make_layout(vec![("_x", "t_uint256", "uint256")]);
        let slither = SlitherReport {
            detectors: vec![Detector {
                check: "reentrancy-eth".to_string(),
                impact: "High".to_string(),
                confidence: "High".to_string(),
                description: "".into(),
            }],
        };
        let report = validate(&manifest, &[layout], &slither).expect("ok");
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("reentrancy-eth"));
    }

    #[test]
    fn low_impact_finding_does_not_warn() {
        let manifest = make_manifest(vec![]);
        let layout = make_layout(vec![("_x", "t_uint256", "uint256")]);
        let slither = SlitherReport {
            detectors: vec![Detector {
                check: "reentrancy-events".into(),
                impact: "Low".into(),
                confidence: "High".into(),
                description: String::new(),
            }],
        };
        let report = validate(&manifest, &[layout], &slither).expect("ok");
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn manifest_without_havoc_does_not_warn() {
        let mut manifest = make_manifest(vec![]);
        manifest.requires.external_call_handling = None;
        let layout = make_layout(vec![("_x", "t_uint256", "uint256")]);
        let slither = SlitherReport {
            detectors: vec![Detector {
                check: "reentrancy-eth".into(),
                impact: "High".into(),
                confidence: "High".into(),
                description: String::new(),
            }],
        };
        let report = validate(&manifest, &[layout], &slither).expect("ok");
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn type_match_tolerates_whitespace() {
        assert!(types_match(
            "mapping(address => uint256)",
            "mapping(address  =>  uint256)"
        ));
        assert!(types_match("uint256", "uint256"));
        assert!(!types_match("uint256", "uint128"));
    }
}
