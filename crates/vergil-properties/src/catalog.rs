//! Catalog types: manifest schema, template loader, structural lint.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse manifest {path}: {source}")]
    Yaml {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("template {id}: encoding file {file} does not exist (under {dir})")]
    MissingEncoding {
        id: String,
        file: String,
        dir: PathBuf,
    },
    #[error("template {id}: path {file} escapes the template directory")]
    EscapingPath { id: String, file: String },
    #[error("template {id}: tier {tier:?} forbids {license} license per SPEC §3.9")]
    LicenseTierViolation {
        id: String,
        tier: Tier,
        license: String,
    },
    #[error("template id {id} appears more than once")]
    DuplicateId { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CostClass {
    Trivial,
    Cheap,
    Medium,
    Hard,
}

impl CostClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            CostClass::Trivial => "trivial",
            CostClass::Cheap => "cheap",
            CostClass::Medium => "medium",
            CostClass::Hard => "hard",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Vendored permissively-licensed sources (OZ MIT, Compound BSD).
    Vendored,
    /// Authored by Vergil, inspired by published patterns but not derivative.
    Original,
    /// Reference-only links to GPL/AGPL/BUSL sources; never vendored.
    ReferenceOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub tier: Tier,
    pub source: String,
    #[serde(default)]
    pub inspired_by: Option<String>,
    /// SPDX-style identifier; e.g. "MIT", "BSD-3-Clause", "Apache-2.0".
    /// Required for vendored content; informational otherwise.
    pub license: String,
    #[serde(default)]
    pub upstream_commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageSlotReq {
    pub name: String,
    /// Solidity type, e.g. `mapping(address => uint256)` or `uint256`.
    #[serde(rename = "type")]
    pub solidity_type: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requires {
    #[serde(default)]
    pub storage_slots: Vec<StorageSlotReq>,
    #[serde(default)]
    pub modifiers: Vec<String>,
    /// How the property handles external calls. Currently only `havoc` is
    /// modeled; future values can be added without breaking older manifests
    /// because the field is a free string with a documented set of values.
    #[serde(default)]
    pub external_call_handling: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliesTo {
    #[serde(default)]
    pub interfaces: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodingPaths {
    pub halmos: String,
    #[serde(default)]
    pub smtchecker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyManifest {
    pub id: String,
    pub description: String,
    pub cost_class: CostClass,
    #[serde(default)]
    pub applies_to: AppliesTo,
    #[serde(default)]
    pub requires: Requires,
    pub encoding: EncodingPaths,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyTemplate {
    pub manifest: PropertyManifest,
    pub dir: PathBuf,
    /// Halmos encoding contents (read from `manifest.encoding.halmos`).
    pub halmos_source: String,
    /// SMTChecker encoding contents. Empty string if the manifest declared
    /// no SMTChecker encoding — Halmos alone discharges the property.
    pub smtchecker_source: String,
}

impl PropertyTemplate {
    /// Stable content hash over manifest id + description + Halmos source.
    /// Used by the retrieval cache to skip re-embedding unchanged templates.
    pub fn content_sha(&self) -> [u8; 32] {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(self.manifest.id.as_bytes());
        h.update(b"\0");
        h.update(self.manifest.description.as_bytes());
        h.update(b"\0");
        h.update(self.halmos_source.as_bytes());
        h.finalize().into()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    by_id: BTreeMap<String, PropertyTemplate>,
}

impl Catalog {
    /// Walk `dir` for `*/manifest.yaml` files, load each template, lint, and
    /// return a populated catalog. Returns the first error encountered.
    pub fn load(dir: impl AsRef<Path>) -> Result<Self, TemplateError> {
        let dir = dir.as_ref();
        let mut by_id: BTreeMap<String, PropertyTemplate> = BTreeMap::new();
        for entry in walk_template_dirs(dir)? {
            let template = load_template(&entry)?;
            lint_template(&template)?;
            if by_id.contains_key(&template.manifest.id) {
                return Err(TemplateError::DuplicateId {
                    id: template.manifest.id.clone(),
                });
            }
            by_id.insert(template.manifest.id.clone(), template);
        }
        Ok(Self { by_id })
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&PropertyTemplate> {
        self.by_id.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &PropertyTemplate> {
        self.by_id.values()
    }
}

fn walk_template_dirs(root: &Path) -> Result<Vec<PathBuf>, TemplateError> {
    let mut out = Vec::new();
    let read = std::fs::read_dir(root).map_err(|e| TemplateError::Io {
        path: root.to_path_buf(),
        source: e,
    })?;
    for entry in read {
        let entry = entry.map_err(|e| TemplateError::Io {
            path: root.to_path_buf(),
            source: e,
        })?;
        let p = entry.path();
        if p.is_dir() && p.join("manifest.yaml").is_file() {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}

fn load_template(dir: &Path) -> Result<PropertyTemplate, TemplateError> {
    let manifest_path = dir.join("manifest.yaml");
    let bytes = std::fs::read(&manifest_path).map_err(|e| TemplateError::Io {
        path: manifest_path.clone(),
        source: e,
    })?;
    let manifest: PropertyManifest =
        serde_yaml::from_slice(&bytes).map_err(|e| TemplateError::Yaml {
            path: manifest_path,
            source: e,
        })?;

    let halmos_rel = &manifest.encoding.halmos;
    let halmos_path =
        resolve_under(dir, halmos_rel).ok_or_else(|| TemplateError::EscapingPath {
            id: manifest.id.clone(),
            file: halmos_rel.clone(),
        })?;
    let halmos_source =
        std::fs::read_to_string(&halmos_path).map_err(|_| TemplateError::MissingEncoding {
            id: manifest.id.clone(),
            file: halmos_rel.clone(),
            dir: dir.to_path_buf(),
        })?;

    let smtchecker_source = match &manifest.encoding.smtchecker {
        Some(rel) => {
            let path = resolve_under(dir, rel).ok_or_else(|| TemplateError::EscapingPath {
                id: manifest.id.clone(),
                file: rel.clone(),
            })?;
            std::fs::read_to_string(&path).map_err(|_| TemplateError::MissingEncoding {
                id: manifest.id.clone(),
                file: rel.clone(),
                dir: dir.to_path_buf(),
            })?
        }
        None => String::new(),
    };

    Ok(PropertyTemplate {
        manifest,
        dir: dir.to_path_buf(),
        halmos_source,
        smtchecker_source,
    })
}

fn resolve_under(dir: &Path, rel: &str) -> Option<PathBuf> {
    if rel.contains("..") || Path::new(rel).is_absolute() {
        return None;
    }
    Some(dir.join(rel))
}

fn lint_template(t: &PropertyTemplate) -> Result<(), TemplateError> {
    let license = t.manifest.provenance.license.to_ascii_uppercase();
    let forbidden_for_vendored = [
        "GPL", "GPL-2.0", "GPL-3.0", "AGPL", "AGPL-3.0", "BUSL", "BUSL-1.1",
    ];
    let is_forbidden = forbidden_for_vendored
        .iter()
        .any(|f| license.starts_with(f));
    if matches!(t.manifest.provenance.tier, Tier::Vendored | Tier::Original) && is_forbidden {
        return Err(TemplateError::LicenseTierViolation {
            id: t.manifest.id.clone(),
            tier: t.manifest.provenance.tier,
            license: t.manifest.provenance.license.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(p: &Path, body: &str) {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, body).unwrap();
    }

    fn minimal_manifest(id: &str, tier: &str, license: &str) -> String {
        format!(
            "id: {id}\ndescription: test\ncost_class: cheap\nencoding:\n  halmos: halmos.sol\nprovenance:\n  tier: {tier}\n  source: test\n  license: {license}\n"
        )
    }

    #[test]
    fn loads_single_template() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("erc20-balance");
        write(
            &dir.join("manifest.yaml"),
            &minimal_manifest("erc20-balance", "original", "Apache-2.0"),
        );
        write(&dir.join("halmos.sol"), "function check_x() public {}");

        let cat = Catalog::load(tmp.path()).expect("load");
        assert_eq!(cat.len(), 1);
        let t = cat.get("erc20-balance").unwrap();
        assert_eq!(t.manifest.cost_class, CostClass::Cheap);
        assert!(t.smtchecker_source.is_empty());
    }

    #[test]
    fn missing_encoding_file_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("bad");
        write(
            &dir.join("manifest.yaml"),
            &minimal_manifest("bad", "original", "Apache-2.0"),
        );
        // halmos.sol intentionally missing
        let err = Catalog::load(tmp.path()).unwrap_err();
        assert!(
            matches!(err, TemplateError::MissingEncoding { .. }),
            "{err}"
        );
    }

    #[test]
    fn escaping_path_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("bad");
        let mut manifest = minimal_manifest("bad", "original", "Apache-2.0");
        manifest = manifest.replace("halmos: halmos.sol", "halmos: ../escape.sol");
        write(&dir.join("manifest.yaml"), &manifest);
        let err = Catalog::load(tmp.path()).unwrap_err();
        assert!(matches!(err, TemplateError::EscapingPath { .. }), "{err}");
    }

    #[test]
    fn gpl_vendored_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("gpl");
        write(
            &dir.join("manifest.yaml"),
            &minimal_manifest("gpl", "vendored", "GPL-3.0"),
        );
        write(&dir.join("halmos.sol"), "function check_x() public {}");
        let err = Catalog::load(tmp.path()).unwrap_err();
        assert!(
            matches!(err, TemplateError::LicenseTierViolation { .. }),
            "{err}"
        );
    }

    #[test]
    fn agpl_original_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("agpl");
        write(
            &dir.join("manifest.yaml"),
            &minimal_manifest("agpl", "original", "AGPL-3.0"),
        );
        write(&dir.join("halmos.sol"), "function check_x() public {}");
        let err = Catalog::load(tmp.path()).unwrap_err();
        assert!(matches!(err, TemplateError::LicenseTierViolation { .. }));
    }

    #[test]
    fn busl_original_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("busl");
        write(
            &dir.join("manifest.yaml"),
            &minimal_manifest("busl", "original", "BUSL-1.1"),
        );
        write(&dir.join("halmos.sol"), "function check_x() public {}");
        let err = Catalog::load(tmp.path()).unwrap_err();
        assert!(matches!(err, TemplateError::LicenseTierViolation { .. }));
    }

    #[test]
    fn reference_only_allows_gpl() {
        // GPL is allowed for Tier 3 (reference-only) because nothing is vendored.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("ref");
        write(
            &dir.join("manifest.yaml"),
            &minimal_manifest("ref", "reference_only", "GPL-3.0"),
        );
        write(&dir.join("halmos.sol"), "function check_x() public {}");
        Catalog::load(tmp.path()).expect("reference-only GPL allowed");
    }

    #[test]
    fn duplicate_id_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        for sub in ["a", "b"] {
            let dir = tmp.path().join(sub);
            write(
                &dir.join("manifest.yaml"),
                &minimal_manifest("dup", "original", "Apache-2.0"),
            );
            write(&dir.join("halmos.sol"), "function check_x() public {}");
        }
        let err = Catalog::load(tmp.path()).unwrap_err();
        assert!(matches!(err, TemplateError::DuplicateId { .. }));
    }

    #[test]
    fn content_sha_changes_with_description_or_source() {
        let mk = |desc: &str, src: &str| PropertyTemplate {
            manifest: PropertyManifest {
                id: "x".into(),
                description: desc.into(),
                cost_class: CostClass::Cheap,
                applies_to: AppliesTo::default(),
                requires: Requires::default(),
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
            },
            dir: PathBuf::new(),
            halmos_source: src.into(),
            smtchecker_source: String::new(),
        };
        assert_ne!(mk("a", "x").content_sha(), mk("b", "x").content_sha());
        assert_ne!(mk("a", "x").content_sha(), mk("a", "y").content_sha());
        assert_eq!(mk("a", "x").content_sha(), mk("a", "x").content_sha());
    }
}
