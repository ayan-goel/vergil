//! Stage 0 — Ingest & fingerprint per SPEC §3.1.
//!
//! Deterministic project ingestion that every later stage of the V1.5
//! Phase 6 standardized workflow consumes. No LLM, sub-second budget,
//! single sweep of the project root: detect interfaces (via
//! `vergil_solidity::signatures::detect_interfaces`), classify the
//! financial primitive with the Phase-1 heuristic (Phase 3 supersedes
//! it later), and report which oracle inputs (tests, NatSpec, README)
//! exist for the contract. Stage 1's three oracles read these signals
//! before deciding what to run.
//!
//! The primitive taxonomy mirrors SPEC §3.3:
//! `token-erc20` / `token-erc721` / `token-erc1155`, `vault`,
//! `lending-market`, `amm`, plus the catch-all `access-controlled-generic`
//! when no specific primitive matches. ERC-4626 vaults intentionally
//! drop the `token-erc20` tag from `primitives` (the vault primitive
//! supersedes the share-token aspect for activation purposes); the
//! ERC20 interface tag is still emitted in `interfaces`.
//!
//! This is NOT the V1 `StaticFacts` activation input — Slice 8's
//! orchestrator bridges Fingerprint → StaticFacts for the catalog
//! activation engine (`vergil_properties::activate`). Keeping the
//! Fingerprint shape clean lets Phase 3 swap the primitive classifier
//! without rewriting catalog code.

use std::path::{Path, PathBuf};

use thiserror::Error;

use vergil_properties::classify::{classify, ClassifyConfig, PrimitiveClassification};
use vergil_solidity::natspec::parse_natspec_dir;
use vergil_solidity::signatures::detect_interfaces;
use vergil_solidity::test_parser::parse_tests;

/// Stage 0 output. Every field is derived from the on-disk project
/// state with no LLM calls. Two calls against the same project return
/// equal Fingerprints (vectors are sorted; booleans are deterministic).
// Note: `Eq` dropped in Phase 3 — `PrimitiveClassification.matches[]`
// carries f32 confidence which doesn't impl Eq. No call site requires
// Eq on Fingerprint (verified via grep).
#[derive(Debug, Clone, PartialEq)]
pub struct Fingerprint {
    /// Canonicalized project root (the dir holding `foundry.toml`).
    pub project_root: PathBuf,
    /// Interface tags detected across the joined `src/` source. Sorted
    /// alphabetically. Always includes nothing implicit — the empty
    /// vec means no recognizable interface was detected (the contract
    /// is generic).
    pub interfaces: Vec<String>,
    /// Financial primitives in SPEC §3.3's taxonomy. Sorted. For ERC-
    /// 4626 contracts this is `["vault"]` (the vault aspect supersedes
    /// the share-token aspect); plain ERC-20 produces `["token-erc20"]`.
    pub primitives: Vec<String>,
    /// What oracle inputs exist on disk for Stage 1 to draw from.
    pub available_oracles: AvailableOracles,
    /// `.sol` files under `src/`, sorted by path. Excludes test files
    /// (those live under `test/`) and inherited libraries (under `lib/`).
    pub contract_sources: Vec<PathBuf>,
    /// V1.5 Phase 3 — full primitive classification with confidence
    /// scores + per-match signal lists. `Fingerprint::primitives` is
    /// derived from this (top-confidence match's id, for backward
    /// compat with Phase 1 consumers). Empty when no signals matched.
    pub primitive_classification: PrimitiveClassification,
}

/// Which Stage-1 oracle inputs the project provides. The booleans
/// don't promise extractor success — they say "an extractor could be
/// run." Slice 8's orchestrator pairs these with the CLI flags
/// (`--no-tests` / `--no-natspec`) to decide what actually runs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AvailableOracles {
    /// At least one Foundry/Halmos test was parsed from `test/` or
    /// `tests/`. False when no test directory exists, when the dir is
    /// empty, or when every file failed to yield a parsed test.
    pub tests: bool,
    /// At least one NatSpec block was parsed from `src/`. Storage-
    /// only NatSpec counts, as does an inherited contract's
    /// annotations if they live in `src/`.
    pub natspec: bool,
    /// Top-level `README.md` (or `README`), if present. The path is
    /// canonicalized.
    pub readme: Option<PathBuf>,
    /// Reserved for the recognized-fork detector (SPEC §3.1 Stage 0).
    /// Always `None` in Phase 6 — the recognized-fork RAG lands in V2.
    /// Kept as `Option<String>` so the seam stays stable.
    pub recognized_fork: Option<String>,
}

#[derive(Debug, Error)]
pub enum FingerprintError {
    /// The path doesn't exist, isn't a directory, or doesn't contain a
    /// `foundry.toml`.
    #[error("not a Foundry project: {0}")]
    NotAProject(PathBuf),
    /// Filesystem-level error reading a file.
    #[error("read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Run Stage 0 against a project root. Returns a deterministic
/// `Fingerprint`. Errors only on filesystem-level problems (missing
/// project, unreadable files) — empty oracles / empty interfaces /
/// empty primitives are valid Fingerprint states.
pub fn fingerprint(project_root: &Path) -> Result<Fingerprint, FingerprintError> {
    let project_root = project_root
        .canonicalize()
        .map_err(|_| FingerprintError::NotAProject(project_root.to_path_buf()))?;
    if !project_root.is_dir() || !project_root.join("foundry.toml").is_file() {
        return Err(FingerprintError::NotAProject(project_root));
    }

    let contract_sources = collect_contract_sources(&project_root)?;
    let joined = join_sources(&contract_sources)?;
    let interfaces = sorted_interfaces(&joined);
    // V1.5 Phase 3 — Real classifier. Source-only (no storage layouts
    // at Stage 0); the bench-measured ≥97% accuracy on source alone
    // means we don't pay the solc per-file cost in the hot fingerprint
    // path. Phase 5's structural pass loads layouts when it needs them.
    let primitive_classification = classify(&joined, &[], &ClassifyConfig::default());
    // Backward-compat: the legacy `primitives` field carries the
    // classifier's top match (or the Phase-1 heuristic as fallback
    // when no match landed). External consumers reading
    // `Fingerprint::primitives` see the same string vec they always
    // did; classifier richness is available via the new
    // `primitive_classification` field.
    let primitives = if let Some(top) = primitive_classification.top() {
        vec![top.primitive.id().to_string()]
    } else {
        detect_primitives(&interfaces, &joined)
    };
    let available_oracles = detect_available_oracles(&project_root)?;

    Ok(Fingerprint {
        project_root,
        interfaces,
        primitives,
        available_oracles,
        contract_sources,
        primitive_classification,
    })
}

fn collect_contract_sources(project_root: &Path) -> Result<Vec<PathBuf>, FingerprintError> {
    let src = project_root.join("src");
    let mut out = Vec::new();
    if src.is_dir() {
        walk_sol(&src, &mut out);
    } else {
        // Single-file layout: shallow scan of project root (skipping
        // sibling dirs like test/, lib/, out/, cache/). Mirrors the
        // natspec/test_parser fallback so a `examples/foo/Foo.sol`
        // layout still produces a fingerprint.
        let Ok(entries) = std::fs::read_dir(project_root) else {
            return Ok(out);
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_file() && p.extension().map(|x| x == "sol").unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn walk_sol(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_sol(&p, out);
        } else if p.extension().map(|x| x == "sol").unwrap_or(false) {
            out.push(p);
        }
    }
}

fn join_sources(sources: &[PathBuf]) -> Result<String, FingerprintError> {
    let mut joined = String::new();
    for path in sources {
        let s = std::fs::read_to_string(path).map_err(|e| FingerprintError::Io {
            path: path.clone(),
            source: e,
        })?;
        joined.push_str(&s);
        joined.push('\n');
    }
    Ok(joined)
}

fn sorted_interfaces(joined: &str) -> Vec<String> {
    let mut set: std::collections::BTreeSet<String> =
        detect_interfaces(joined).into_iter().collect();

    // Supplemental detection — `detect_interfaces` only inspects
    // function declarations, so `mapping(...) public allowance` (which
    // solc auto-getters into `allowance(address,address) view`) is
    // invisible. Recognize the common storage shapes that imply an
    // interface; this lifts the Phase-1 heuristic from
    // `commands/catalog::collect_facts` so behavior is unchanged.
    let has_public_allowance = joined.contains("public allowance");
    let has_public_balanceof = joined.contains("public balanceOf")
        || joined.contains("public balances")
        || joined.contains("balanceOf[");
    let has_function_transfer = joined.contains("function transfer(");
    let has_function_transferfrom = joined.contains("function transferFrom(");
    let has_erc4626_shape = joined.contains("convertToShares")
        || joined.contains("convertToAssets")
        || (joined.contains("totalAssets") && joined.contains("totalShares"));
    let has_erc721_shape = joined.contains("ownerOf")
        && (joined.contains("safeTransferFrom") || joined.contains("setApprovalForAll"));
    let has_erc1155_shape =
        joined.contains("safeBatchTransferFrom") && joined.contains("balanceOfBatch");

    if has_function_transfer
        && has_function_transferfrom
        && has_public_allowance
        && !has_erc721_shape
    {
        set.insert("ERC20".to_string());
    }
    if has_public_balanceof && has_function_transfer && has_public_allowance && !has_erc721_shape {
        set.insert("ERC20".to_string());
    }
    if has_erc721_shape {
        set.insert("ERC721".to_string());
    }
    if has_erc1155_shape {
        set.insert("ERC1155".to_string());
    }
    if has_erc4626_shape {
        set.insert("ERC4626".to_string());
        set.insert("ERC20".to_string()); // vault is also an ERC-20 share token
    }
    set.into_iter().collect()
}

/// Phase-1 primitive heuristic. The Phase 3 classifier
/// (`vergil-properties::classify`) will supersede this with confidence
/// scores and multi-primitive matches; until then, single-primary
/// detection from interfaces + a small set of function-name fingerprints.
///
/// Ordering matters: vault supersedes the share-token aspect of an
/// ERC-4626 contract — activation rules for vault attacks should fire
/// on an ERC-4626 contract, while ERC-20 attacks fire via the
/// `interfaces` tag. This mirrors SPEC §3.3's taxonomy.
fn detect_primitives(interfaces: &[String], joined: &str) -> Vec<String> {
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    let has = |needle: &str| interfaces.iter().any(|s| s == needle);

    if has("ERC4626") {
        out.insert("vault".to_string());
    } else if has("ERC20") {
        out.insert("token-erc20".to_string());
    } else if has("ERC721") {
        out.insert("token-erc721".to_string());
    } else if has("ERC1155") {
        out.insert("token-erc1155".to_string());
    }

    // Function-name fingerprints for primitives outside the ERC-x
    // family. Conservative — a contract that has both ERC-20 and
    // lending function names is treated as both (multi-primitive is
    // allowed by SPEC §3.3; Phase 3 adds confidence scoring).
    let has_lending_shape = joined.contains("function borrow(")
        && joined.contains("function repay(")
        && joined.contains("function liquidate(");
    let has_amm_shape = joined.contains("function swap(")
        && (joined.contains("reserves0") || joined.contains("reserve0"));

    if has_lending_shape {
        out.insert("lending-market".to_string());
    }
    if has_amm_shape {
        out.insert("amm".to_string());
    }

    out.into_iter().collect()
}

fn detect_available_oracles(project_root: &Path) -> Result<AvailableOracles, FingerprintError> {
    // Tests — `parse_tests` returns Ok(empty) on a project with no
    // test/ dir, so any IO error here is a real problem worth
    // surfacing.
    let tests = match parse_tests(project_root) {
        Ok(parsed) => !parsed.is_empty(),
        Err(vergil_solidity::test_parser::TestParserError::NotADirectory(_)) => false,
        Err(vergil_solidity::test_parser::TestParserError::Io { path, source }) => {
            return Err(FingerprintError::Io { path, source })
        }
    };

    // NatSpec — same treatment as tests.
    let natspec = match parse_natspec_dir(project_root) {
        Ok(blocks) => !blocks.is_empty(),
        Err(vergil_solidity::natspec::NatSpecParserError::NotADirectory(_)) => false,
        Err(vergil_solidity::natspec::NatSpecParserError::Io { path, source }) => {
            return Err(FingerprintError::Io { path, source })
        }
    };

    let readme = ["README.md", "README", "readme.md"]
        .iter()
        .map(|n| project_root.join(n))
        .find(|p| p.is_file());

    Ok(AvailableOracles {
        tests,
        natspec,
        readme,
        recognized_fork: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn examples_root() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop(); // crates/vergil-core
        p.pop(); // crates
        p.push("examples");
        p
    }

    #[test]
    fn fingerprint_errors_on_missing_project() {
        let bogus = PathBuf::from("/this/path/does/not/exist/vergil-fp-test");
        let err = fingerprint(&bogus).unwrap_err();
        assert!(
            matches!(err, FingerprintError::NotAProject(_)),
            "expected NotAProject for missing path, got {err:?}"
        );
    }

    #[test]
    fn fingerprint_errors_on_dir_without_foundry_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let err = fingerprint(tmp.path()).unwrap_err();
        assert!(
            matches!(err, FingerprintError::NotAProject(_)),
            "expected NotAProject for dir without foundry.toml, got {err:?}"
        );
    }

    #[test]
    fn fingerprint_erc20_example() {
        let project = examples_root().join("erc20");
        let fp = fingerprint(&project).expect("erc20 fingerprint");
        assert!(
            fp.interfaces.contains(&"ERC20".to_string()),
            "expected ERC20 interface tag, got {:?}",
            fp.interfaces
        );
        assert_eq!(
            fp.primitives,
            vec!["token-erc20".to_string()],
            "expected primitives=[token-erc20], got {:?}",
            fp.primitives
        );
        assert!(
            fp.available_oracles.tests,
            "erc20 has test/Properties.t.sol"
        );
        assert!(
            !fp.contract_sources.is_empty(),
            "expected at least one .sol source"
        );
    }

    #[test]
    fn fingerprint_vault_4626_example_classifies_as_vault() {
        let project = examples_root().join("vault-4626");
        let fp = fingerprint(&project).expect("vault-4626 fingerprint");
        assert!(
            fp.interfaces.contains(&"ERC4626".to_string()),
            "expected ERC4626 interface, got {:?}",
            fp.interfaces
        );
        assert!(
            fp.interfaces.contains(&"ERC20".to_string()),
            "expected ERC20 interface (share token), got {:?}",
            fp.interfaces
        );
        assert_eq!(
            fp.primitives,
            vec!["vault".to_string()],
            "vault supersedes the share-token aspect: expected [vault], got {:?}",
            fp.primitives
        );
        assert!(fp.available_oracles.tests, "vault-4626 has test/");
        // Phase 4 added @notice/@invariant/@custom:security to vault-4626.
        assert!(
            fp.available_oracles.natspec,
            "vault-4626 has Phase-4 NatSpec annotations"
        );
    }

    #[test]
    fn fingerprint_erc721_example_classifies_as_token_erc721() {
        let project = examples_root().join("erc721");
        let fp = fingerprint(&project).expect("erc721 fingerprint");
        assert!(
            fp.interfaces.contains(&"ERC721".to_string()),
            "expected ERC721 interface, got {:?}",
            fp.interfaces
        );
        assert_eq!(
            fp.primitives,
            vec!["token-erc721".to_string()],
            "expected primitives=[token-erc721], got {:?}",
            fp.primitives
        );
    }

    #[test]
    fn fingerprint_lending_example_detects_lending_market() {
        let project = examples_root().join("lending");
        let fp = fingerprint(&project).expect("lending fingerprint");
        assert!(
            fp.primitives.contains(&"lending-market".to_string()),
            "expected lending-market in primitives, got {:?}",
            fp.primitives
        );
        assert!(fp.available_oracles.tests, "lending has test/");
    }

    #[test]
    fn fingerprint_is_deterministic_across_calls() {
        let project = examples_root().join("vault-4626");
        let a = fingerprint(&project).expect("first call");
        let b = fingerprint(&project).expect("second call");
        assert_eq!(a, b, "fingerprint must be deterministic");
    }

    #[test]
    fn detect_primitives_empty_interfaces_returns_empty() {
        let primitives = detect_primitives(&[], "");
        assert!(primitives.is_empty());
    }

    #[test]
    fn detect_primitives_amm_shape() {
        let src = "function swap(uint a, uint b) public {} uint reserves0;";
        let primitives = detect_primitives(&[], src);
        assert_eq!(primitives, vec!["amm".to_string()]);
    }
}
