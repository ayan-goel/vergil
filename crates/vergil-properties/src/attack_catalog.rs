//! Attack-pattern catalog: schema, loader, activation rules.
//!
//! Sibling of [`crate::catalog`] (the V1 conformance catalog). Where the
//! conformance catalog encodes positive properties ("balances sum to total
//! supply"), the attack catalog encodes the *negation* of known attack
//! patterns — what must hold for the attack to fail. Each template ships:
//!
//!   * `manifest.yaml`  — per [`AttackManifest`] (SPEC §3.2.1).
//!   * `halmos.sol.tmpl`  — Halmos `check_*` encoding (Slice-1 decision:
//!     required).
//!   * `smtchecker.sol.tmpl`  — SMTChecker invariant (Slice-1 decision:
//!     optional for decidable patterns; warned on absence; required for
//!     CHC-only patterns).
//!   * `fixtures/vulnerable.sol`  — planted-bug contract that the template's
//!     negation MUST refute (Halmos → Counterexample). Structural defense
//!     against template bugs (SPEC §9.1).
//!   * `fixtures/clean.sol`  — mitigated contract that the template's
//!     negation MUST verify (Halmos → Verified).
//!   * `mutations.yaml`  — placeholder schema in Phase 1; real Gambit
//!     mutation coverage in Phase 2 mid-checkpoint.
//!   * `README.md`  — one paragraph; manifest references.
//!
//! [`AttackCatalog::load`] walks `templates/attacks/*/manifest.yaml`, parses
//! each, reads the four source files (two encodings + two fixtures), and
//! runs [`lint_attack_template`] which catches:
//!
//!   - missing or escaping encoding/fixture file paths,
//!   - frontier patterns missing an explicit `over_approximation`,
//!   - duplicate IDs across templates,
//!   - GPL/AGPL/BUSL-licensed content vendored under Tier 1 / Tier 2 (SPEC
//!     §3.9 — Vergil distributes the catalog as part of the binary).
//!
//! Activation: [`activate`] takes [`StaticFacts`] extracted from a contract
//! (interfaces, primitives, pattern flags) and returns the subset of
//! templates whose `applies_to` predicates ALL hold. Non-matches surface as
//! [`SkippedTemplate`] so coverage is auditable (SPEC §3.6 "Not checked").

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::catalog::{StorageSlotReq, Tier};

// ─── Severity ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Informational,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Informational => "informational",
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
            Severity::Critical => "critical",
        }
    }
}

// ─── Decidability ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SmtStatus {
    Decidable,
    Frontier,
    DocumentOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedSolver {
    Z3,
    Cvc5,
    Either,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedTheory {
    /// Linear integer arithmetic.
    Lia,
    /// Bitvector.
    Bv,
    /// Uninterpreted functions.
    Uf,
    /// Mixed / portfolio across the above.
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decidability {
    pub smt_status: SmtStatus,
    /// REQUIRED when `smt_status == Frontier`. Plain-English description of
    /// the sound over-approximation the template actually ships. See SPEC
    /// §0.3 and `notes/attack-patterns.md` §0.3.
    #[serde(default)]
    pub over_approximation: Option<String>,
    pub expected_solver: ExpectedSolver,
    pub expected_theory: ExpectedTheory,
}

// ─── Applicability ───────────────────────────────────────────────────────────

/// Activation predicate over a contract's static facts. `interfaces` and
/// `primitives` are sets of tag strings the template applies to; `[any]`
/// matches everything. `patterns` is a map of pattern-flag names → required
/// boolean; ALL entries must match the corresponding [`StaticFacts`] flag.
///
/// **Schema deviation from SPEC §3.2.1 example.** The SPEC YAML shows
/// `patterns` and `modifiers` as lists of single-key maps:
///
/// ```yaml
/// patterns:
///   - external_call_present: true
///   - state_change_after_external_call: true
/// ```
///
/// We accept (and prefer in our authored manifests) the plain-map form,
/// which deserializes directly into [`BTreeMap`] without a custom visitor:
///
/// ```yaml
/// patterns:
///   external_call_present: true
///   state_change_after_external_call: true
/// ```
///
/// Documented as a Phase-1 schema decision; SPEC §3.2.1's example is updated
/// in the Slice-7 closeout pass.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliesTo {
    #[serde(default)]
    pub interfaces: Vec<String>,
    #[serde(default)]
    pub primitives: Vec<String>,
    #[serde(default)]
    pub patterns: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ModifierPresence {
    Required,
    Optional,
    AbsentRequired,
}

/// See [`AppliesTo`] for the schema-deviation note — modifiers also accept the
/// plain-map form: `{ nonReentrant: optional }`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackRequires {
    #[serde(default)]
    pub storage_slots: Vec<StorageSlotReq>,
    #[serde(default)]
    pub modifiers: BTreeMap<String, ModifierPresence>,
    #[serde(default)]
    pub external_call_handling: Option<String>,
}

// ─── Encoding & fixtures ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackEncoding {
    /// Required. Halmos `check_*` template (`.sol.tmpl`).
    pub halmos: String,
    /// Optional for decidable patterns; required for CHC-only / unbounded
    /// patterns (a load-time warning fires when absent + status decidable).
    #[serde(default)]
    pub smtchecker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackFixtures {
    /// Planted-bug Solidity file under the template dir; the template's
    /// negation property MUST refute it (Halmos → Counterexample). Release
    /// blocker (SPEC §9.1).
    pub vulnerable: String,
    /// Mitigated Solidity file under the template dir; the template's
    /// negation property MUST verify (Halmos → Verified).
    pub clean: String,
}

// ─── Provenance ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealWorldExploit {
    pub name: String,
    #[serde(default)]
    pub year: Option<u32>,
    #[serde(default)]
    pub loss_usd_approx: Option<u64>,
    #[serde(default)]
    pub chain: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackProvenance {
    pub tier: Tier,
    pub source: String,
    /// SPDX identifier; same Tier-1/Tier-2 license discipline as
    /// [`crate::catalog::Provenance`].
    pub license: String,
    #[serde(default)]
    pub references: Vec<String>,
    #[serde(default)]
    pub real_world: Vec<RealWorldExploit>,
}

// ─── The manifest ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackManifest {
    pub id: String,
    pub name: String,
    /// Category tag matching one of the 16 categories in SPEC §3.2.2.
    pub category: String,
    pub severity: Severity,
    pub decidability: Decidability,
    #[serde(default)]
    pub applies_to: AppliesTo,
    #[serde(default)]
    pub requires: AttackRequires,
    /// Plain English (first paragraph) + quasi-formal notation (second
    /// paragraph). The heart of the entry per `notes/attack-patterns.md` §0.1.
    pub negation_property: String,
    /// Required for `decidable` and `frontier` templates. Optional for
    /// `document_only` templates (which ship as manifest + README only —
    /// the catalog-as-data §11.3 model).
    #[serde(default)]
    pub encoding: Option<AttackEncoding>,
    /// Required for `decidable` and `frontier` templates. Optional for
    /// `document_only` templates.
    #[serde(default)]
    pub fixtures: Option<AttackFixtures>,
    pub provenance: AttackProvenance,
    pub mitigation: String,
    #[serde(default)]
    pub engineering_notes: Option<String>,
}

// ─── Loaded template ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttackTemplate {
    pub manifest: AttackManifest,
    pub dir: PathBuf,
    /// Halmos encoding contents (read from `manifest.encoding.halmos`).
    pub halmos_source: String,
    /// SMTChecker encoding contents. Empty string when the manifest declared
    /// no SMTChecker encoding (Halmos alone discharges the property).
    pub smtchecker_source: String,
    pub vulnerable_source: String,
    pub clean_source: String,
}

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AttackError {
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
    #[error("attack {id}: encoding file {file} does not exist (under {dir})")]
    MissingEncoding {
        id: String,
        file: String,
        dir: PathBuf,
    },
    #[error("attack {id}: fixture file {file} does not exist (under {dir})")]
    MissingFixture {
        id: String,
        file: String,
        dir: PathBuf,
    },
    #[error("attack {id}: path {file} escapes the template directory")]
    EscapingPath { id: String, file: String },
    #[error("attack id {id} appears more than once")]
    DuplicateId { id: String },
    #[error(
        "attack {id}: smt_status=frontier requires `decidability.over_approximation` in manifest"
    )]
    FrontierMissingOverApprox { id: String },
    #[error("attack {id}: tier {tier:?} forbids {license} license per SPEC §3.9")]
    LicenseTierViolation {
        id: String,
        tier: Tier,
        license: String,
    },
}

// ─── Catalog ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct AttackCatalog {
    by_id: BTreeMap<String, AttackTemplate>,
}

impl AttackCatalog {
    /// Walk `dir` for `*/manifest.yaml`, parse + lint, return a populated
    /// catalog. Returns the first error encountered.
    pub fn load(dir: impl AsRef<Path>) -> Result<Self, AttackError> {
        let dir = dir.as_ref();
        let mut by_id: BTreeMap<String, AttackTemplate> = BTreeMap::new();
        for entry in walk_template_dirs(dir)? {
            let template = load_template(&entry)?;
            lint_attack_template(&template)?;
            if by_id.contains_key(&template.manifest.id) {
                return Err(AttackError::DuplicateId {
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

    pub fn get(&self, id: &str) -> Option<&AttackTemplate> {
        self.by_id.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &AttackTemplate> {
        self.by_id.values()
    }
}

// ─── Static facts + activation ───────────────────────────────────────────────

/// Static-analysis facts extracted from a contract — the input to
/// [`activate`]. Phase 1 populates these from the existing
/// `vergil-solidity::signatures::detect_interfaces` + a small primitive
/// heuristic + basic pattern-flag extraction. Phase 3 replaces the primitive
/// heuristic with a real classifier.
#[derive(Debug, Clone, Default)]
pub struct StaticFacts {
    pub interfaces: BTreeSet<String>,
    pub primitives: BTreeSet<String>,
    pub patterns: BTreeMap<String, bool>,
}

impl StaticFacts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_interface(mut self, i: impl Into<String>) -> Self {
        self.interfaces.insert(i.into());
        self
    }

    pub fn with_primitive(mut self, p: impl Into<String>) -> Self {
        self.primitives.insert(p.into());
        self
    }

    pub fn with_pattern(mut self, k: impl Into<String>, v: bool) -> Self {
        self.patterns.insert(k.into(), v);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedTemplate {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Default, Clone)]
pub struct ActivationResult<'a> {
    pub templates: Vec<&'a AttackTemplate>,
    pub skipped: Vec<SkippedTemplate>,
}

/// Return the subset of templates in `catalog` whose `applies_to` predicates
/// ALL hold against `facts`. Templates that don't activate are recorded as
/// [`SkippedTemplate`] with a human-readable reason so the report can list
/// them under "Not checked" (SPEC §3.6).
pub fn activate<'a>(catalog: &'a AttackCatalog, facts: &StaticFacts) -> ActivationResult<'a> {
    let mut out = ActivationResult::default();
    for t in catalog.iter() {
        match matches_applies_to(&t.manifest.applies_to, facts) {
            Ok(()) => out.templates.push(t),
            Err(reason) => out.skipped.push(SkippedTemplate {
                id: t.manifest.id.clone(),
                reason,
            }),
        }
    }
    out
}

fn matches_applies_to(a: &AppliesTo, facts: &StaticFacts) -> Result<(), String> {
    // interfaces: `[any]` matches everything; else any-of intersection.
    if !a.interfaces.is_empty() && !contains_any_tag(&a.interfaces) {
        let intersects = a.interfaces.iter().any(|t| facts.interfaces.contains(t));
        if !intersects {
            return Err(format!(
                "no overlap with required interfaces {:?}",
                a.interfaces
            ));
        }
    }
    // primitives: same rule.
    if !a.primitives.is_empty() && !contains_any_tag(&a.primitives) {
        let intersects = a.primitives.iter().any(|t| facts.primitives.contains(t));
        if !intersects {
            return Err(format!(
                "no overlap with required primitives {:?}",
                a.primitives
            ));
        }
    }
    // patterns: every required key must match the fact.
    for (k, want) in &a.patterns {
        let got = facts.patterns.get(k).copied().unwrap_or(false);
        if got != *want {
            return Err(format!("pattern {k}={want} not satisfied (got {got})"));
        }
    }
    Ok(())
}

fn contains_any_tag(tags: &[String]) -> bool {
    tags.iter().any(|t| t == "any")
}

// ─── Loader internals ────────────────────────────────────────────────────────

fn walk_template_dirs(root: &Path) -> Result<Vec<PathBuf>, AttackError> {
    let mut out = Vec::new();
    let read = std::fs::read_dir(root).map_err(|e| AttackError::Io {
        path: root.to_path_buf(),
        source: e,
    })?;
    for entry in read {
        let entry = entry.map_err(|e| AttackError::Io {
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

fn load_template(dir: &Path) -> Result<AttackTemplate, AttackError> {
    let manifest_path = dir.join("manifest.yaml");
    let bytes = std::fs::read(&manifest_path).map_err(|e| AttackError::Io {
        path: manifest_path.clone(),
        source: e,
    })?;
    let manifest: AttackManifest =
        serde_yaml::from_slice(&bytes).map_err(|e| AttackError::Yaml {
            path: manifest_path,
            source: e,
        })?;

    let is_document_only = matches!(manifest.decidability.smt_status, SmtStatus::DocumentOnly);

    // Encoding: required for decidable + frontier, skipped for document_only.
    let (halmos_source, smtchecker_source) = match (&manifest.encoding, is_document_only) {
        (Some(enc), _) => {
            let h = read_under(dir, &manifest.id, &enc.halmos, false)?;
            let s = match &enc.smtchecker {
                Some(rel) => read_under(dir, &manifest.id, rel, false)?,
                None => {
                    if matches!(manifest.decidability.smt_status, SmtStatus::Decidable) {
                        tracing::warn!(
                            attack_id = %manifest.id,
                            "decidable attack template ships no SMTChecker encoding; running Halmos-only"
                        );
                    }
                    String::new()
                }
            };
            (h, s)
        }
        (None, true) => (String::new(), String::new()),
        (None, false) => {
            return Err(AttackError::MissingEncoding {
                id: manifest.id.clone(),
                file: "encoding section in manifest".to_string(),
                dir: dir.to_path_buf(),
            });
        }
    };

    // Fixtures: required for decidable + frontier, skipped for document_only.
    let (vulnerable_source, clean_source) = match (&manifest.fixtures, is_document_only) {
        (Some(fx), _) => {
            let v = read_under(dir, &manifest.id, &fx.vulnerable, true)?;
            let c = read_under(dir, &manifest.id, &fx.clean, true)?;
            (v, c)
        }
        (None, true) => (String::new(), String::new()),
        (None, false) => {
            return Err(AttackError::MissingFixture {
                id: manifest.id.clone(),
                file: "fixtures section in manifest".to_string(),
                dir: dir.to_path_buf(),
            });
        }
    };

    Ok(AttackTemplate {
        manifest,
        dir: dir.to_path_buf(),
        halmos_source,
        smtchecker_source,
        vulnerable_source,
        clean_source,
    })
}

/// `is_fixture` switches the error variant on a missing file (so the lint
/// distinguishes a missing encoding from a missing fixture).
fn read_under(dir: &Path, id: &str, rel: &str, is_fixture: bool) -> Result<String, AttackError> {
    let path = resolve_under(dir, rel).ok_or_else(|| AttackError::EscapingPath {
        id: id.to_string(),
        file: rel.to_string(),
    })?;
    std::fs::read_to_string(&path).map_err(|_| {
        if is_fixture {
            AttackError::MissingFixture {
                id: id.to_string(),
                file: rel.to_string(),
                dir: dir.to_path_buf(),
            }
        } else {
            AttackError::MissingEncoding {
                id: id.to_string(),
                file: rel.to_string(),
                dir: dir.to_path_buf(),
            }
        }
    })
}

fn resolve_under(dir: &Path, rel: &str) -> Option<PathBuf> {
    if rel.contains("..") || Path::new(rel).is_absolute() {
        return None;
    }
    Some(dir.join(rel))
}

fn lint_attack_template(t: &AttackTemplate) -> Result<(), AttackError> {
    // Frontier patterns must declare their over-approximation.
    if matches!(t.manifest.decidability.smt_status, SmtStatus::Frontier)
        && t.manifest
            .decidability
            .over_approximation
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(AttackError::FrontierMissingOverApprox {
            id: t.manifest.id.clone(),
        });
    }

    // Tier 1 / Tier 2 license discipline (matches catalog.rs lint).
    let license = t.manifest.provenance.license.to_ascii_uppercase();
    let forbidden_for_vendored = [
        "GPL", "GPL-2.0", "GPL-3.0", "AGPL", "AGPL-3.0", "BUSL", "BUSL-1.1",
    ];
    let is_forbidden = forbidden_for_vendored
        .iter()
        .any(|f| license.starts_with(f));
    if matches!(t.manifest.provenance.tier, Tier::Vendored | Tier::Original) && is_forbidden {
        return Err(AttackError::LicenseTierViolation {
            id: t.manifest.id.clone(),
            tier: t.manifest.provenance.tier,
            license: t.manifest.provenance.license.clone(),
        });
    }

    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

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

    fn minimal_manifest(id: &str, status: &str, extra_decidability: &str) -> String {
        format!(
            r#"id: {id}
name: Test attack
category: access
severity: high
decidability:
  smt_status: {status}
  expected_solver: z3
  expected_theory: uf
{extra_decidability}
applies_to:
  interfaces: [any]
negation_property: |
  Test property.
encoding:
  halmos: halmos.sol.tmpl
fixtures:
  vulnerable: fixtures/vulnerable.sol
  clean: fixtures/clean.sol
provenance:
  tier: original
  source: vergil-test
  license: Apache-2.0
mitigation: Test mitigation.
"#
        )
    }

    fn write_full_template(root: &Path, id: &str, status: &str, extra_dec: &str) {
        let dir = root.join(id);
        write(
            &dir.join("manifest.yaml"),
            &minimal_manifest(id, status, extra_dec),
        );
        write(
            &dir.join("halmos.sol.tmpl"),
            "// halmos\nfunction check_x() public {}\n",
        );
        write(&dir.join("fixtures/vulnerable.sol"), "contract V {}\n");
        write(&dir.join("fixtures/clean.sol"), "contract C {}\n");
    }

    // ─── schema deser ────────────────────────────────────────────────────────

    #[test]
    fn manifest_deserializes_minimal() {
        let y = minimal_manifest("a", "decidable", "");
        let m: AttackManifest = serde_yaml::from_str(&y).expect("parse minimal");
        assert_eq!(m.id, "a");
        assert_eq!(m.category, "access");
        assert_eq!(m.severity, Severity::High);
        assert_eq!(m.decidability.smt_status, SmtStatus::Decidable);
        assert_eq!(m.decidability.expected_theory, ExpectedTheory::Uf);
        // (snake_case rename: `UF` in fixture YAML is `uf`-rejected by serde.)
        assert!(m.encoding.as_ref().unwrap().smtchecker.is_none());
    }

    #[test]
    fn manifest_deserializes_frontier_with_overapprox() {
        let y = minimal_manifest(
            "a",
            "frontier",
            "  over_approximation: |\n    A weaker decidable property.\n",
        );
        let m: AttackManifest = serde_yaml::from_str(&y).expect("parse");
        assert_eq!(m.decidability.smt_status, SmtStatus::Frontier);
        assert!(m
            .decidability
            .over_approximation
            .as_ref()
            .unwrap()
            .contains("decidable"));
    }

    #[test]
    fn pattern_map_syntax_round_trips() {
        let y = r#"
interfaces: [ERC20]
primitives: [token]
patterns:
  external_call_present: true
  state_change_after_external_call: false
"#;
        let a: AppliesTo = serde_yaml::from_str(y).unwrap();
        assert_eq!(a.patterns.get("external_call_present"), Some(&true));
        assert_eq!(
            a.patterns.get("state_change_after_external_call"),
            Some(&false)
        );
    }

    // ─── loader ──────────────────────────────────────────────────────────────

    #[test]
    fn loads_single_template() {
        let tmp = tempfile::tempdir().unwrap();
        write_full_template(tmp.path(), "a", "decidable", "");
        let cat = AttackCatalog::load(tmp.path()).expect("load");
        assert_eq!(cat.len(), 1);
        let t = cat.get("a").unwrap();
        assert_eq!(t.manifest.severity, Severity::High);
        assert!(t.smtchecker_source.is_empty());
        assert_eq!(t.vulnerable_source.trim(), "contract V {}");
        assert_eq!(t.clean_source.trim(), "contract C {}");
    }

    #[test]
    fn missing_halmos_encoding_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("a");
        write(
            &dir.join("manifest.yaml"),
            &minimal_manifest("a", "decidable", ""),
        );
        write(&dir.join("fixtures/vulnerable.sol"), "contract V {}");
        write(&dir.join("fixtures/clean.sol"), "contract C {}");
        // halmos.sol.tmpl intentionally absent
        let err = AttackCatalog::load(tmp.path()).unwrap_err();
        assert!(matches!(err, AttackError::MissingEncoding { .. }), "{err}");
    }

    #[test]
    fn missing_fixture_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("a");
        write(
            &dir.join("manifest.yaml"),
            &minimal_manifest("a", "decidable", ""),
        );
        write(&dir.join("halmos.sol.tmpl"), "// halmos");
        write(&dir.join("fixtures/clean.sol"), "contract C {}");
        // vulnerable.sol intentionally absent
        let err = AttackCatalog::load(tmp.path()).unwrap_err();
        assert!(matches!(err, AttackError::MissingFixture { .. }), "{err}");
    }

    /// Minimal manifest for a `document_only` template — the SPEC §11.3
    /// catalog-as-data branch: no encoding, no fixtures, manifest only.
    fn document_only_manifest(id: &str) -> String {
        format!(
            r#"id: {id}
name: Test document-only attack
category: access
severity: high
decidability:
  smt_status: document_only
  expected_solver: z3
  expected_theory: uf
applies_to:
  interfaces: [any]
negation_property: |
  Documented; no auto-verification.
provenance:
  tier: original
  source: vergil-test
  license: Apache-2.0
mitigation: Test mitigation.
"#
        )
    }

    #[test]
    fn loads_document_only_template_without_encoding_or_fixtures() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("doc-only");
        write(
            &dir.join("manifest.yaml"),
            &document_only_manifest("doc-only"),
        );
        // No halmos.sol.tmpl, no fixtures/*.sol — and the load must succeed.
        let cat = AttackCatalog::load(tmp.path()).expect("document-only loads");
        assert_eq!(cat.len(), 1);
        let t = cat.get("doc-only").unwrap();
        assert_eq!(t.manifest.decidability.smt_status, SmtStatus::DocumentOnly);
        assert!(t.halmos_source.is_empty());
        assert!(t.smtchecker_source.is_empty());
        assert!(t.vulnerable_source.is_empty());
        assert!(t.clean_source.is_empty());
        assert!(t.manifest.encoding.is_none());
        assert!(t.manifest.fixtures.is_none());
    }

    #[test]
    fn decidable_without_encoding_still_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("a");
        // decidable manifest but no encoding section
        let manifest = r#"id: a
name: Test attack
category: access
severity: high
decidability:
  smt_status: decidable
  expected_solver: z3
  expected_theory: uf
applies_to:
  interfaces: [any]
negation_property: |
  Test.
provenance:
  tier: original
  source: vergil-test
  license: Apache-2.0
mitigation: Test.
"#;
        write(&dir.join("manifest.yaml"), manifest);
        let err = AttackCatalog::load(tmp.path()).unwrap_err();
        assert!(matches!(err, AttackError::MissingEncoding { .. }), "{err}");
    }

    #[test]
    fn escaping_path_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("a");
        let m = minimal_manifest("a", "decidable", "")
            .replace("halmos: halmos.sol.tmpl", "halmos: ../escape.sol");
        write(&dir.join("manifest.yaml"), &m);
        let err = AttackCatalog::load(tmp.path()).unwrap_err();
        assert!(matches!(err, AttackError::EscapingPath { .. }), "{err}");
    }

    #[test]
    fn duplicate_id_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        write_full_template(tmp.path(), "dup-a", "decidable", "");
        write_full_template(tmp.path(), "dup-b", "decidable", "");
        // Rewrite second template's manifest with the first's id.
        let m_a = minimal_manifest("dup", "decidable", "");
        write(&tmp.path().join("dup-a/manifest.yaml"), &m_a);
        write(&tmp.path().join("dup-b/manifest.yaml"), &m_a);
        let err = AttackCatalog::load(tmp.path()).unwrap_err();
        assert!(matches!(err, AttackError::DuplicateId { .. }), "{err}");
    }

    #[test]
    fn frontier_missing_overapprox_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        // status=frontier but no over_approximation field
        write_full_template(tmp.path(), "a", "frontier", "");
        let err = AttackCatalog::load(tmp.path()).unwrap_err();
        assert!(
            matches!(err, AttackError::FrontierMissingOverApprox { .. }),
            "{err}"
        );
    }

    #[test]
    fn frontier_with_overapprox_loads() {
        let tmp = tempfile::tempdir().unwrap();
        write_full_template(
            tmp.path(),
            "a",
            "frontier",
            "  over_approximation: |\n    Weaker decidable form.\n",
        );
        AttackCatalog::load(tmp.path()).expect("load");
    }

    #[test]
    fn gpl_original_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        write_full_template(tmp.path(), "a", "decidable", "");
        let m = minimal_manifest("a", "decidable", "").replace("Apache-2.0", "GPL-3.0");
        write(&tmp.path().join("a/manifest.yaml"), &m);
        let err = AttackCatalog::load(tmp.path()).unwrap_err();
        assert!(
            matches!(err, AttackError::LicenseTierViolation { .. }),
            "{err}"
        );
    }

    // ─── activation ──────────────────────────────────────────────────────────

    fn catalog_with(applies_to_yaml: &str) -> AttackCatalog {
        let tmp = tempfile::tempdir().unwrap();
        let id = "test-attack";
        let m = format!(
            r#"id: {id}
name: T
category: access
severity: high
decidability:
  smt_status: decidable
  expected_solver: z3
  expected_theory: uf
applies_to:
{applies_to_yaml}
negation_property: P
encoding:
  halmos: halmos.sol.tmpl
fixtures:
  vulnerable: fixtures/vulnerable.sol
  clean: fixtures/clean.sol
provenance:
  tier: original
  source: t
  license: Apache-2.0
mitigation: M
"#
        );
        let dir = tmp.path().join(id);
        write(&dir.join("manifest.yaml"), &m);
        write(&dir.join("halmos.sol.tmpl"), "// h");
        write(&dir.join("fixtures/vulnerable.sol"), "contract V {}");
        write(&dir.join("fixtures/clean.sol"), "contract C {}");
        let cat = AttackCatalog::load(tmp.path()).expect("load");
        std::mem::forget(tmp); // keep dir alive for catalog references
        cat
    }

    #[test]
    fn activation_any_interface_matches_everything() {
        let cat = catalog_with("  interfaces: [any]\n");
        let result = activate(&cat, &StaticFacts::new());
        assert_eq!(result.templates.len(), 1);
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn activation_interface_intersection_required() {
        let cat = catalog_with("  interfaces: [ERC20]\n");
        let no_match = activate(&cat, &StaticFacts::new().with_interface("ERC721"));
        assert_eq!(no_match.templates.len(), 0);
        assert_eq!(no_match.skipped.len(), 1);
        assert!(no_match.skipped[0].reason.contains("ERC20"));

        let match_ = activate(&cat, &StaticFacts::new().with_interface("ERC20"));
        assert_eq!(match_.templates.len(), 1);
    }

    #[test]
    fn activation_primitive_intersection_required() {
        let cat = catalog_with("  primitives: [vault]\n");
        let no = activate(&cat, &StaticFacts::new().with_primitive("amm"));
        assert_eq!(no.templates.len(), 0);

        let yes = activate(&cat, &StaticFacts::new().with_primitive("vault"));
        assert_eq!(yes.templates.len(), 1);
    }

    #[test]
    fn activation_pattern_all_must_match() {
        let cat =
            catalog_with("  interfaces: [any]\n  patterns:\n    external_call_present: true\n");
        // Pattern absent → defaults to false → does not match the required true.
        let no = activate(&cat, &StaticFacts::new());
        assert_eq!(no.templates.len(), 0);
        assert!(no.skipped[0].reason.contains("external_call_present"));

        let yes = activate(
            &cat,
            &StaticFacts::new().with_pattern("external_call_present", true),
        );
        assert_eq!(yes.templates.len(), 1);
    }
}
