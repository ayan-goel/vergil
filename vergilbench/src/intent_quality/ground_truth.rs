//! Ground-truth loader for the intent-quality overlay.
//!
//! Reads every `<corpus>/contracts/<name>/properties.yaml` and extracts:
//!   - the top-level `intent:` string (one English sentence)
//!   - the `properties[].name` list (Halmos `check_*` function names)
//!   - the optional `provenance:` block (OZ contracts carry source +
//!     commit + license metadata)
//!
//! The loader is forgiving: a contract missing `properties.yaml` or
//! with a malformed yaml is logged and skipped rather than failing the
//! whole pass. Only the corpus root being absent is fatal.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

/// One contract's hand-written ground truth, as the bench author wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchGroundTruth {
    /// The top-level `intent:` string. Single English sentence describing
    /// the contract's invariant.
    pub intent_text: String,
    /// Names of the hand-written `check_*` properties under `properties:`.
    /// Slice 4 uses these for sub-property coverage attribution.
    pub property_names: Vec<String>,
    /// Source/version/commit/license, when present. Carried for the retro
    /// only — the overlay scorer doesn't use it.
    pub provenance: Option<Provenance>,
}

/// Optional `provenance:` block from the OpenZeppelin bench contracts.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Provenance {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
}

/// Load all ground-truth entries under `<corpus>/contracts/`.
///
/// Returns a map of contract directory name → `BenchGroundTruth`.
/// Contracts whose `properties.yaml` is missing, unreadable, malformed,
/// or has an empty `intent:` field are logged to stderr and excluded
/// from the returned map.
pub fn load(corpus_dir: &Path) -> Result<HashMap<String, BenchGroundTruth>, String> {
    let contracts_dir = corpus_dir.join("contracts");
    if !contracts_dir.is_dir() {
        return Err(format!(
            "no contracts directory at {}",
            contracts_dir.display()
        ));
    }

    let entries = std::fs::read_dir(&contracts_dir)
        .map_err(|e| format!("read {}: {e}", contracts_dir.display()))?;

    let mut out = HashMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        match load_one(&path) {
            Ok(Some(gt)) => {
                out.insert(name, gt);
            }
            Ok(None) => {
                // load_one already logged the reason.
            }
            Err(e) => {
                eprintln!("[intent-quality] skip {name}: {e}");
            }
        }
    }

    Ok(out)
}

/// Load a single contract's ground truth. Returns `Ok(None)` for the
/// expected "skip this contract" cases (missing yaml, empty intent);
/// `Err` only on filesystem errors that aren't expected by the corpus
/// invariant.
fn load_one(contract_dir: &Path) -> Result<Option<BenchGroundTruth>, String> {
    let yaml_path = contract_dir.join("properties.yaml");
    if !yaml_path.is_file() {
        eprintln!(
            "[intent-quality] skip {}: no properties.yaml",
            contract_dir.display()
        );
        return Ok(None);
    }

    let body = std::fs::read_to_string(&yaml_path)
        .map_err(|e| format!("read {}: {e}", yaml_path.display()))?;

    let parsed: PropertiesFile = match serde_yaml::from_str(&body) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "[intent-quality] skip {}: yaml parse error on {}: {e}",
                contract_dir.display(),
                yaml_path.display()
            );
            return Ok(None);
        }
    };

    let intent_text = parsed.intent.unwrap_or_default().trim().to_string();
    if intent_text.is_empty() {
        eprintln!(
            "[intent-quality] skip {}: missing or empty `intent:` field",
            contract_dir.display()
        );
        return Ok(None);
    }

    let property_names = parsed.properties.into_iter().map(|p| p.name).collect();

    Ok(Some(BenchGroundTruth {
        intent_text,
        property_names,
        provenance: parsed.provenance,
    }))
}

#[derive(Debug, Deserialize)]
struct PropertiesFile {
    #[serde(default)]
    intent: Option<String>,
    #[serde(default)]
    properties: Vec<PropertyEntry>,
    #[serde(default)]
    provenance: Option<Provenance>,
}

#[derive(Debug, Deserialize)]
struct PropertyEntry {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_corpus(td: &TempDir, fixtures: &[(&str, &str)]) {
        let contracts = td.path().join("contracts");
        for (name, yaml) in fixtures {
            let dir = contracts.join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("properties.yaml"), yaml).unwrap();
        }
    }

    #[test]
    fn loads_three_contract_corpus() {
        let td = TempDir::new().unwrap();
        write_corpus(
            &td,
            &[
                (
                    "erc20",
                    r#"
version: 1
intent: "Transfers conserve balances and total supply."
properties:
  - name: check_transfer_preserves_supply
  - name: check_approve_idempotent
"#,
                ),
                (
                    "vault",
                    r#"
version: 1
intent: "Deposits and withdrawals preserve totalAssets."
properties:
  - name: check_deposit_mints_shares
"#,
                ),
                (
                    "oz-base64",
                    r#"
version: 1
intent: "Encoding empty input yields empty string."
provenance:
  source: openzeppelin-contracts
  version: 5.1.0
  commit: 69c8def5f222ff96f2b5beff05dfba996368aa79
  license: MIT
properties:
  - name: check_empty_encodes_empty
"#,
                ),
            ],
        );

        let m = load(td.path()).unwrap();
        assert_eq!(m.len(), 3);

        let erc20 = m.get("erc20").unwrap();
        assert_eq!(
            erc20.intent_text,
            "Transfers conserve balances and total supply."
        );
        assert_eq!(
            erc20.property_names,
            vec![
                "check_transfer_preserves_supply".to_string(),
                "check_approve_idempotent".to_string(),
            ]
        );
        assert!(erc20.provenance.is_none());

        let oz = m.get("oz-base64").unwrap();
        let prov = oz.provenance.as_ref().expect("provenance");
        assert_eq!(prov.source.as_deref(), Some("openzeppelin-contracts"));
        assert_eq!(prov.version.as_deref(), Some("5.1.0"));
        assert_eq!(prov.license.as_deref(), Some("MIT"));
    }

    #[test]
    fn skips_malformed_yaml_without_failing_the_pass() {
        let td = TempDir::new().unwrap();
        write_corpus(
            &td,
            &[
                (
                    "good",
                    r#"
version: 1
intent: "Sound contract."
properties:
  - name: check_ok
"#,
                ),
                ("broken", "this is: : not : valid: yaml [oops"),
            ],
        );
        let m = load(td.path()).unwrap();
        assert_eq!(m.len(), 1);
        assert!(m.contains_key("good"));
        assert!(!m.contains_key("broken"));
    }

    #[test]
    fn skips_contracts_with_empty_intent() {
        let td = TempDir::new().unwrap();
        write_corpus(
            &td,
            &[
                (
                    "has_intent",
                    "version: 1\nintent: \"non-empty\"\nproperties: []",
                ),
                (
                    "blank_intent",
                    "version: 1\nintent: \"   \"\nproperties: []",
                ),
                ("no_intent", "version: 1\nproperties: []"),
            ],
        );
        let m = load(td.path()).unwrap();
        assert_eq!(m.len(), 1);
        assert!(m.contains_key("has_intent"));
    }

    #[test]
    fn errors_on_missing_corpus() {
        let td = TempDir::new().unwrap();
        let err = load(td.path()).unwrap_err();
        assert!(err.contains("no contracts directory"));
    }

    #[test]
    fn handles_contract_without_properties_yaml() {
        let td = TempDir::new().unwrap();
        fs::create_dir_all(td.path().join("contracts/lonely")).unwrap();
        // No properties.yaml in lonely/
        let m = load(td.path()).unwrap();
        assert_eq!(m.len(), 0);
    }

    /// Sanity check against the real 100-contract bench corpus. Ignored
    /// by default so it doesn't run in unit-test fast-path; invoke with
    /// `cargo test -p vergilbench --lib loads_real_bench_corpus --
    /// --ignored --nocapture` when calibrating Slice 3's taxonomy
    /// against the real ground-truth strings.
    #[test]
    #[ignore]
    fn loads_real_bench_corpus() {
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let m = load(&workspace_root).expect("load bench corpus");
        eprintln!("loaded {} ground-truth entries", m.len());
        assert_eq!(
            m.len(),
            100,
            "bench corpus should have all 100 valid ground-truth entries; saw {}",
            m.len()
        );
        // Spot-check three known-present contracts
        for k in ["erc20", "oz-erc20-basic", "oz-base64"] {
            assert!(m.contains_key(k), "expected ground-truth entry for {k}");
        }
    }
}
