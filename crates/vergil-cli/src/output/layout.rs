//! Centralized path-builder for the `vergil-out/` artifact tree.
//!
//! V1.5 Phase 6 Slice 4 — see SPEC §3.8. Every writer in the CLI calls
//! through one helper here rather than building paths inline so the
//! tree shape stays stable across slices (Slice 6 streams cex, Slice 8
//! unifies orchestration, Slice 11 updates `vergil prove`).
//!
//! Target layout:
//!
//! ```text
//! vergil-out/
//! ├── report.md                            # stratified verdict (Slice 5)
//! ├── proof.json                           # canonical, all verified properties
//! ├── zero-config/
//! │   ├── attack-catalog/<attack-id>.proof.json
//! │   ├── conformance/
//! │   ├── tests/
//! │   ├── natspec/
//! │   └── structural/
//! ├── intent/
//! ├── counterexamples/                     # Cex_<property>.t.sol (kept top-level)
//! ├── smt/                                 # SMT-LIB queries (V1)
//! └── trace/run.jsonl                      # structured log (V1)
//! ```
//!
//! Backward compatibility: V1 / Phase-1 artifacts live at
//! `vergil-out/proof.json` and `vergil-out/counterexamples/`. Those
//! paths are preserved — Slice 4 ADDS subordinate per-tier directories
//! without disturbing the existing roots.

use std::io;
use std::path::{Path, PathBuf};

use vergil_proof::schema::{Source, Tier};

/// Project-rooted `vergil-out/` directory. Always
/// `<project>/vergil-out/`.
pub fn vergil_out(project: &Path) -> PathBuf {
    project.join("vergil-out")
}

/// Canonical proof artifact path. The top-level `proof.json` lists
/// every verified property (Phase 6 SPEC §3.6 "Proven" section).
/// `vergil prove` reads from this path by default.
pub fn top_level_proof_json(project: &Path) -> PathBuf {
    vergil_out(project).join("proof.json")
}

/// Stratified verdict report (Slice 5). Human-readable companion to
/// `proof.json`. Slice 5 writes here.
#[allow(dead_code)]
pub fn report_md(project: &Path) -> PathBuf {
    vergil_out(project).join("report.md")
}

/// Counterexample directory. Cex files stay top-level (not under a
/// tier subdirectory) so the existing Phase-1 path
/// `vergil-out/counterexamples/Cex_<property>.t.sol` remains stable.
/// Slice 6 streams cex files here; Slice 8's verdict uses the same
/// location.
pub fn counterexamples_dir(project: &Path) -> PathBuf {
    vergil_out(project).join("counterexamples")
}

/// SMT-LIB query persistence directory (V1).
pub fn smt_dir(project: &Path) -> PathBuf {
    vergil_out(project).join("smt")
}

/// Per-run telemetry log path (V1). Slice 8's orchestrator will route
/// telemetry writes through this; until then it stays defined so the
/// path shape is locked.
#[allow(dead_code)]
pub fn trace_jsonl(project: &Path) -> PathBuf {
    vergil_out(project).join("trace").join("run.jsonl")
}

/// Stage-2 confirmation gate state (Slice 7). On-disk persistence
/// so a killed run can resume via `--resume`.
#[allow(dead_code)]
pub fn confirm_state(project: &Path) -> PathBuf {
    vergil_out(project).join("confirm").join("state.json")
}

/// Tier-aware subdirectory. For Phase 6, every Stage-1 oracle write
/// lands under `zero-config/<source>/`; intent-tier writes land
/// under `intent/`. The top-level `proof.json` is the canonical
/// roll-up; these subordinate files exist for per-oracle audit /
/// debugging.
pub fn tier_dir(project: &Path, tier: Tier) -> PathBuf {
    match tier {
        Tier::ZeroConfig => vergil_out(project).join("zero-config"),
        Tier::Intent => vergil_out(project).join("intent"),
    }
}

/// Per-source subdirectory inside a tier. For zero-config:
/// `attack-catalog/`, `conformance/`, `tests/`, `natspec/`,
/// `structural/`. For intent: just the tier dir (no further split).
pub fn source_dir(project: &Path, tier: Tier, source: Source) -> PathBuf {
    let base = tier_dir(project, tier);
    match (tier, source) {
        (Tier::ZeroConfig, Source::AttackCatalog) => base.join("attack-catalog"),
        (Tier::ZeroConfig, Source::Conformance) => base.join("conformance"),
        (Tier::ZeroConfig, Source::Tests) => base.join("tests"),
        (Tier::ZeroConfig, Source::NatSpec) => base.join("natspec"),
        (Tier::ZeroConfig, Source::Structural) => base.join("structural"),
        // Source::UserIntent under ZeroConfig is anomalous (UserIntent
        // is the V1 intent path); fall through to the tier dir.
        (Tier::ZeroConfig, Source::UserIntent) => base,
        (Tier::Intent, _) => base,
    }
}

/// Per-template subordinate proof for an attack-catalog source.
/// Phase-1's catalog self-test only emitted one top-level summary;
/// Phase 6 keeps the per-template file as an audit aid alongside the
/// canonical top-level `proof.json`. Example:
/// `vergil-out/zero-config/attack-catalog/reentrancy-single-function-cei.proof.json`.
///
/// Slice 8 uses this from the unified runner; Slice 4 just locks the
/// shape.
#[allow(dead_code)]
pub fn attack_catalog_per_template_proof(project: &Path, attack_id: &str) -> PathBuf {
    source_dir(project, Tier::ZeroConfig, Source::AttackCatalog)
        .join(format!("{attack_id}.proof.json"))
}

/// Create the entire Phase 6 tier-aware tree under `<project>/vergil-out/`.
/// Idempotent — safe to call multiple times in a single run.
/// Returns an error only on real IO problems; pre-existing dirs are
/// fine.
pub fn ensure_tree(project: &Path) -> io::Result<()> {
    let root = vergil_out(project);
    std::fs::create_dir_all(&root)?;
    std::fs::create_dir_all(counterexamples_dir(project))?;
    std::fs::create_dir_all(smt_dir(project))?;
    std::fs::create_dir_all(root.join("trace"))?;
    // Zero-config tier + every Phase 6 source subdir.
    std::fs::create_dir_all(source_dir(project, Tier::ZeroConfig, Source::AttackCatalog))?;
    std::fs::create_dir_all(source_dir(project, Tier::ZeroConfig, Source::Conformance))?;
    std::fs::create_dir_all(source_dir(project, Tier::ZeroConfig, Source::Tests))?;
    std::fs::create_dir_all(source_dir(project, Tier::ZeroConfig, Source::NatSpec))?;
    std::fs::create_dir_all(source_dir(project, Tier::ZeroConfig, Source::Structural))?;
    // Intent tier dir.
    std::fs::create_dir_all(tier_dir(project, Tier::Intent))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> PathBuf {
        PathBuf::from("/tmp/test-project")
    }

    #[test]
    fn top_level_proof_json_path_is_stable() {
        assert_eq!(
            top_level_proof_json(&project()),
            PathBuf::from("/tmp/test-project/vergil-out/proof.json")
        );
    }

    #[test]
    fn counterexamples_dir_stays_at_top_level() {
        assert_eq!(
            counterexamples_dir(&project()),
            PathBuf::from("/tmp/test-project/vergil-out/counterexamples")
        );
    }

    #[test]
    fn source_dir_routes_each_zero_config_source_to_its_subdir() {
        let p = project();
        assert_eq!(
            source_dir(&p, Tier::ZeroConfig, Source::AttackCatalog),
            PathBuf::from("/tmp/test-project/vergil-out/zero-config/attack-catalog")
        );
        assert_eq!(
            source_dir(&p, Tier::ZeroConfig, Source::Tests),
            PathBuf::from("/tmp/test-project/vergil-out/zero-config/tests")
        );
        assert_eq!(
            source_dir(&p, Tier::ZeroConfig, Source::NatSpec),
            PathBuf::from("/tmp/test-project/vergil-out/zero-config/natspec")
        );
        assert_eq!(
            source_dir(&p, Tier::ZeroConfig, Source::Conformance),
            PathBuf::from("/tmp/test-project/vergil-out/zero-config/conformance")
        );
        assert_eq!(
            source_dir(&p, Tier::ZeroConfig, Source::Structural),
            PathBuf::from("/tmp/test-project/vergil-out/zero-config/structural")
        );
    }

    #[test]
    fn intent_tier_uses_intent_dir() {
        assert_eq!(
            tier_dir(&project(), Tier::Intent),
            PathBuf::from("/tmp/test-project/vergil-out/intent")
        );
        assert_eq!(
            source_dir(&project(), Tier::Intent, Source::UserIntent),
            PathBuf::from("/tmp/test-project/vergil-out/intent")
        );
    }

    #[test]
    fn attack_catalog_per_template_path_is_predictable() {
        assert_eq!(
            attack_catalog_per_template_proof(&project(), "reentrancy-cei"),
            PathBuf::from(
                "/tmp/test-project/vergil-out/zero-config/attack-catalog/reentrancy-cei.proof.json"
            )
        );
    }

    #[test]
    fn ensure_tree_creates_full_layout_idempotently() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        ensure_tree(project).expect("first ensure_tree");
        // Idempotent — second call must succeed.
        ensure_tree(project).expect("second ensure_tree");
        // Every directory listed in SPEC §3.8 must exist.
        assert!(vergil_out(project).is_dir());
        assert!(counterexamples_dir(project).is_dir());
        assert!(smt_dir(project).is_dir());
        for s in [
            Source::AttackCatalog,
            Source::Conformance,
            Source::Tests,
            Source::NatSpec,
            Source::Structural,
        ] {
            assert!(
                source_dir(project, Tier::ZeroConfig, s).is_dir(),
                "missing zero-config source dir: {s:?}"
            );
        }
        assert!(tier_dir(project, Tier::Intent).is_dir());
    }
}
