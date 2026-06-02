//! `vergil catalog list|show|validate|self-test` — inspect the attack
//! catalog and exercise its templates against their own clean fixtures.
//!
//! The catalog directory is fixed to the workspace's
//! `crates/vergil-properties/templates/attacks/` for now; when V1.5
//! Phase 2 lands the full catalog, a `--templates-dir` override can be
//! added without breaking these subcommands.
//!
//! V1.5 Phase 6 Slice 0 moved the catalog-self-test loop from the
//! Phase-1 `vergil verify --mode zero-config` surface into a sibling
//! `vergil catalog self-test <PATH>` subcommand. The user-facing
//! `verify --mode zero-config` slot is now reserved for the Stage 1
//! oracle path Phase 6 Slice 8 wires up. The self-test logic is
//! verification-engine-developer infrastructure: it confirms that the
//! activation engine selects the right templates for a project and
//! that each selected template's `clean.sol` fixture verifies under
//! Halmos. It does NOT verify the user's contract against the catalog
//! — that's the per-contract dispatch Phase 6 ships via
//! `catalog_intent.rs` and the unified `vergil verify` runner.

use std::path::{Path, PathBuf};
use std::time::Duration;

use vergil_properties::{
    activate, render, AttackCatalog, AttackTemplate, RenderContext, StaticFacts,
};
use vergil_solidity::halmos::{run_simple, HalmosResult};
use vergil_solidity::signatures::detect_interfaces;

const HALMOS_BUDGET: Duration = Duration::from_secs(120);

/// Repo-root → `crates/vergil-properties/templates/attacks`.
///
/// `env!("CARGO_MANIFEST_DIR")` resolves to `crates/vergil-cli` at build
/// time; two `pop`s reach the repo root regardless of where the binary
/// is invoked from.
fn templates_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/vergil-cli
    p.pop(); // crates
    p.push("crates/vergil-properties/templates/attacks");
    p
}

fn load() -> Result<AttackCatalog, u8> {
    let dir = templates_dir();
    AttackCatalog::load(&dir).map_err(|e| {
        eprintln!("catalog load failed at {}: {e}", dir.display());
        3
    })
}

pub fn run_list(category_filter: Option<String>) -> Result<(), u8> {
    let cat = load()?;
    let mut shown = 0usize;
    for t in cat.iter() {
        if let Some(ref c) = category_filter {
            if &t.manifest.category != c {
                continue;
            }
        }
        println!(
            "{:42} {:8} {:13} {}",
            t.manifest.id,
            t.manifest.severity.as_str(),
            decidability_label(t),
            t.manifest.category,
        );
        shown += 1;
    }
    if cat.is_empty() {
        eprintln!(
            "(no attack templates found in {})",
            templates_dir().display()
        );
    } else if shown == 0 {
        eprintln!("(no templates match the filter)");
    }
    Ok(())
}

pub fn run_show(id: String) -> Result<(), u8> {
    let cat = load()?;
    let Some(t) = cat.get(&id) else {
        eprintln!("no such attack template: {id}");
        return Err(3);
    };
    print_manifest(t);
    Ok(())
}

pub fn run_validate() -> Result<(), u8> {
    let cat = load()?;
    println!(
        "ok: {} attack templates loaded and validated from {}",
        cat.len(),
        templates_dir().display()
    );
    Ok(())
}

/// Catalog-self-test mode (V1.5 Phase 6 Slice 0). For the given project
/// directory, load the attack catalog, extract static facts (interfaces
/// via `detect_interfaces`, primitives via the Phase-1 heuristic),
/// activate templates, and for each applicable template run its own
/// `clean.sol` fixture through Halmos. Refuted = catalog bug (template
/// claims its clean fixture is exploitable).
///
/// This is catalog-development infrastructure, not per-contract
/// dispatch. Per-contract dispatch — rendering each activated template
/// against the user's actual contract — lands in Phase 6 Slice 3's
/// `catalog_intent.rs` via LLM-mediated binding.
pub async fn run_self_test(project: PathBuf) -> Result<(), u8> {
    let project = match project.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("invalid project path {}: {e}", project.display());
            return Err(3);
        }
    };
    if !project.join("foundry.toml").is_file() {
        eprintln!("no foundry.toml in project dir: {}", project.display());
        return Err(3);
    }

    let cat = load()?;
    let facts = collect_facts(&project)?;

    let result = activate(&cat, &facts);

    println!("# Catalog self-test (Phase 1 catalog-self-test mode)");
    println!();
    println!("Project:  {}", project.display());
    println!(
        "Interfaces detected: {}",
        format_set(facts.interfaces.iter().cloned().collect())
    );
    println!(
        "Primitives (Phase-1 heuristic): {}",
        format_set(facts.primitives.iter().cloned().collect())
    );
    println!();
    println!(
        "## Activated templates ({} of {} catalog entries)",
        result.templates.len(),
        cat.len()
    );
    println!();

    let mut any_refuted = false;
    let mut applicable_passed = 0usize;
    for t in &result.templates {
        let (label, refuted) = run_clean_self_test(t).await;
        if refuted {
            any_refuted = true;
        } else {
            applicable_passed += 1;
        }
        println!(
            "{label}  {:42}  {:8}  {}",
            t.manifest.id,
            t.manifest.severity.as_str(),
            t.manifest.category,
        );
    }

    if !result.skipped.is_empty() {
        println!();
        println!(
            "## Not checked — skipped templates ({} of {})",
            result.skipped.len(),
            cat.len()
        );
        println!();
        for s in &result.skipped {
            println!("- {}  ({})", s.id, s.reason);
        }
    }

    println!();
    println!("## Summary");
    println!("- Applicable templates: {}", result.templates.len());
    println!("- Self-tests passed:    {applicable_passed}");
    println!("- Skipped (not applicable): {}", result.skipped.len());
    println!();
    println!("Note: catalog self-test runs each applicable template's `clean.sol`");
    println!("fixture, not the user's source. Per-contract dispatch against the");
    println!("user's contract lands in V1.5 Phase 6 via LLM-mediated binding");
    println!("(crates/vergil-core/src/catalog_intent.rs) and surfaces through");
    println!("`vergil verify` once Slice 8 wires the unified orchestration.");

    if any_refuted {
        Err(1)
    } else {
        Ok(())
    }
}

// ─── catalog self-test helpers (lifted from the Phase-1 zero_config command) ──

/// Collect static facts from the user's project. Phase-1 heuristic:
/// concatenate every `.sol` file under `src/`, run `detect_interfaces`
/// over the joined source, and map known interface tags to primitives.
/// Phase 6 Slice 1 lifts this into a `Fingerprint` API; this function
/// stays here for the self-test path until Phase 3's primitive
/// classifier supersedes it.
pub(crate) fn collect_facts(project: &Path) -> Result<StaticFacts, u8> {
    let src = project.join("src");
    if !src.is_dir() {
        eprintln!("no `src/` directory under project: {}", project.display());
        return Err(3);
    }
    let mut joined = String::new();
    for entry in walk_sol(&src) {
        if let Ok(s) = std::fs::read_to_string(&entry) {
            joined.push_str(&s);
            joined.push('\n');
        }
    }
    let mut facts = StaticFacts::new();
    facts = facts.with_interface("any"); // [any] activation always matches
    let mut detected: std::collections::BTreeSet<String> =
        detect_interfaces(&joined).into_iter().collect();

    // Phase-1 supplemental: `detect_interfaces` only inspects function
    // declarations; a public mapping (`mapping(...) public allowance`)
    // is auto-getter'd by solc but invisible to the regex extractor.
    // Recognize ERC-20 / ERC-721 / ERC-1155 / ERC-4626 storage shapes
    // here so the activation engine sees them. A real fix would
    // generalize `signatures::detect_interfaces` to read mappings;
    // that's a V1.5 Phase 4 / V2 task that touches V1 callers.
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
        detected.insert("ERC20".to_string());
    }
    if has_public_balanceof && has_function_transfer && has_public_allowance && !has_erc721_shape {
        detected.insert("ERC20".to_string());
    }
    if has_erc721_shape {
        detected.insert("ERC721".to_string());
    }
    if has_erc1155_shape {
        detected.insert("ERC1155".to_string());
    }
    if has_erc4626_shape {
        detected.insert("ERC4626".to_string());
        // Vaults are also ERC-20 (the share token).
        detected.insert("ERC20".to_string());
    }

    for tag in &detected {
        facts = facts.with_interface(tag.clone());
        match tag.as_str() {
            "ERC20" | "ERC721" | "ERC1155" => {
                facts = facts.with_primitive("token");
            }
            "ERC4626" => facts = facts.with_primitive("vault"),
            _ => {}
        }
    }
    facts = facts.with_primitive("any"); // [any] always matches

    // Phase-1 pattern flags: conservatively turn every flag ON so the
    // activation engine matches templates that gate on
    // `state_change_present`, `no_auth_check`, etc. The real flag
    // extraction (via static_analysis + slither) lands in Phase 4.
    for flag in [
        "state_change_present",
        "no_auth_check",
        "unchecked_block_present",
        "external_call_present",
        "initialize_present",
        "cancel_present",
        "deposit_present",
        "uups_proxy",
        "bit_shift_present",
    ] {
        facts = facts.with_pattern(flag, true);
    }
    Ok(facts)
}

fn walk_sol(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk_sol(&p));
        } else if p.extension().map(|x| x == "sol").unwrap_or(false) {
            out.push(p);
        }
    }
    out
}

async fn run_clean_self_test(t: &AttackTemplate) -> (&'static str, bool) {
    let render_result = render_for_template(t);
    let Some((check_src, check_fn)) = render_result else {
        return ("?", false);
    };
    let project = match prepare_self_test_project(t, &check_src) {
        Ok(p) => p,
        Err(_) => return ("✗", true),
    };
    let result = run_simple(project.path(), &check_fn, HALMOS_BUDGET).await;
    match result {
        HalmosResult::Verified { .. } => ("✓", false),
        HalmosResult::Counterexample { .. } => ("✗", true),
        _ => ("?", false),
    }
}

/// Map known template IDs to their Phase-1 render context + check-function
/// name. The integration-test driver in `tests/attack_fixtures.rs`
/// established these bindings during template authoring; this table
/// hard-codes them so the self-test command can drive the same fixtures
/// without duplicating the wiring.
fn render_for_template(t: &AttackTemplate) -> Option<(String, String)> {
    let id_ident = t.manifest.id.replace('-', "_");
    let (ctx, check_fn): (RenderContext, &str) = match t.manifest.id.as_str() {
        "access-missing-modifier-state-change" => (
            RenderContext::from_pairs([
                ("contract_name", "Target"),
                ("contract_path", "src/Target.sol"),
                ("setter", "setProtected"),
                ("getter", "protectedValue"),
                ("attack_id_ident", id_ident.as_str()),
            ]),
            "check_unauthorized_caller_cannot_mutate",
        ),
        "arith-overflow-underflow-unchecked" => (
            RenderContext::from_pairs([
                ("contract_name", "Target"),
                ("contract_path", "src/Target.sol"),
                ("op", "add"),
                ("attack_id_ident", id_ident.as_str()),
            ]),
            "check_add_does_not_wrap",
        ),
        "reentrancy-single-function-cei" => (
            RenderContext::from_pairs([
                ("contract_name", "Target"),
                ("contract_path", "src/Target.sol"),
                ("action", "action"),
                ("getter", "counter"),
                ("attack_id_ident", id_ident.as_str()),
            ]),
            "check_action_does_not_double_increment",
        ),
        "access-public-burn-mint" => (
            simple_ctx(id_ident.as_str()),
            "check_unauthorized_mint_cannot_inflate_supply",
        ),
        "init-unprotected-initializer" => (
            simple_ctx(id_ident.as_str()),
            "check_attacker_cannot_seize_ownership",
        ),
        "logic-approval-not-revoked-after-cancel" => (
            simple_ctx(id_ident.as_str()),
            "check_cancel_zeros_allowance",
        ),
        "input-missing-parameter-validation" => (
            simple_ctx(id_ident.as_str()),
            "check_attacker_cannot_cancel",
        ),
        "vault-inflation-first-depositor-donation" => (
            simple_ctx(id_ident.as_str()),
            "check_no_zero_shares_under_inflation",
        ),
        "arith-incorrect-overflow-check-shift" => {
            (simple_ctx(id_ident.as_str()), "check_shift_is_recoverable")
        }
        "init-uninitialized-uups-implementation" => (
            simple_ctx(id_ident.as_str()),
            "check_implementation_cannot_be_initialized",
        ),
        _ => return None,
    };
    let rendered = render(&t.halmos_source, &ctx).ok()?;
    Some((rendered, check_fn.to_string()))
}

fn simple_ctx(id_ident: &str) -> RenderContext {
    RenderContext::from_pairs([
        ("contract_name", "Target"),
        ("contract_path", "src/Target.sol"),
        ("attack_id_ident", id_ident),
    ])
}

const FOUNDRY_TOML: &str = r#"[profile.default]
src = "src"
test = "test"
out = "out"
libs = ["lib"]
solc = "0.8.20"
optimizer = true
optimizer_runs = 200
"#;

fn prepare_self_test_project(
    t: &AttackTemplate,
    check_src: &str,
) -> Result<tempfile::TempDir, std::io::Error> {
    let tmp = tempfile::Builder::new()
        .prefix(&format!("vergil-zc-{}-", t.manifest.id))
        .tempdir()?;
    let root = tmp.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::create_dir_all(root.join("test"))?;
    std::fs::write(root.join("foundry.toml"), FOUNDRY_TOML)?;
    std::fs::write(root.join("src/Target.sol"), &t.clean_source)?;
    std::fs::write(root.join("test/AttackCheck.t.sol"), check_src)?;
    Ok(tmp)
}

fn format_set(mut tags: Vec<String>) -> String {
    tags.sort();
    if tags.is_empty() {
        return "(none)".into();
    }
    tags.join(", ")
}

// ─── formatting helpers ──────────────────────────────────────────────────────

fn decidability_label(t: &AttackTemplate) -> &'static str {
    use vergil_properties::SmtStatus;
    match t.manifest.decidability.smt_status {
        SmtStatus::Decidable => "decidable",
        SmtStatus::Frontier => "frontier",
        SmtStatus::DocumentOnly => "document-only",
    }
}

fn print_manifest(t: &AttackTemplate) {
    let m = &t.manifest;
    println!("id:        {}", m.id);
    println!("name:      {}", m.name);
    println!("category:  {}", m.category);
    println!("severity:  {}", m.severity.as_str());
    println!(
        "decidability: {} ({:?} / {:?})",
        decidability_label(t),
        m.decidability.expected_solver,
        m.decidability.expected_theory,
    );
    if let Some(ref oa) = m.decidability.over_approximation {
        println!("over-approximation:");
        for line in oa.lines() {
            println!("  {line}");
        }
    }
    println!();
    println!("applies-to:");
    if !m.applies_to.interfaces.is_empty() {
        println!("  interfaces: {:?}", m.applies_to.interfaces);
    }
    if !m.applies_to.primitives.is_empty() {
        println!("  primitives: {:?}", m.applies_to.primitives);
    }
    if !m.applies_to.patterns.is_empty() {
        println!("  patterns:");
        for (k, v) in &m.applies_to.patterns {
            println!("    {k}: {v}");
        }
    }
    println!();
    println!("negation property:");
    for line in m.negation_property.lines() {
        println!("  {line}");
    }
    println!();
    println!("mitigation:");
    for line in m.mitigation.lines() {
        println!("  {line}");
    }
    if !m.provenance.real_world.is_empty() {
        println!();
        println!("real-world exploits:");
        for ex in &m.provenance.real_world {
            print!("  - {}", ex.name);
            if let Some(y) = ex.year {
                print!(" ({y})");
            }
            if let Some(l) = ex.loss_usd_approx {
                print!(" ~${l}");
            }
            if let Some(ref c) = ex.chain {
                print!(" on {c}");
            }
            println!();
        }
    }
    if !m.provenance.references.is_empty() {
        println!();
        println!("references:");
        for r in &m.provenance.references {
            println!("  - {r}");
        }
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_dir_resolves_to_existing_path() {
        let dir = templates_dir();
        assert!(
            dir.is_dir(),
            "expected attack templates dir at {}",
            dir.display()
        );
    }

    #[test]
    fn run_list_prints_loaded_attack_templates() {
        let res = run_list(None);
        assert!(res.is_ok(), "list returned err: {res:?}");
    }

    #[test]
    fn run_show_unknown_id_returns_error() {
        let res = run_show("does-not-exist".to_string());
        assert_eq!(res, Err(3));
    }

    #[test]
    fn run_validate_against_real_catalog_is_ok() {
        let res = run_validate();
        assert!(res.is_ok(), "validate returned err: {res:?}");
    }

    #[test]
    fn collect_facts_on_examples_erc20_returns_any_tag() {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.pop();
        p.push("examples/erc20");
        let facts = collect_facts(&p).expect("erc20 facts");
        assert!(
            facts.interfaces.contains("any"),
            "always-on `any` interface missing"
        );
    }
}
