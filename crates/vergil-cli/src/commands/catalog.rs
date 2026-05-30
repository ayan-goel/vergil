//! `vergil catalog list|show|validate` — inspect the attack catalog.
//!
//! Phase-1 surface (SPEC §3.7). The catalog directory is fixed to the
//! workspace's `crates/vergil-properties/templates/attacks/` for now;
//! when V1.5 Phase 2 lands the full ~97 templates, a `--templates-dir`
//! override can be added without breaking these subcommands.

use std::path::PathBuf;

use vergil_properties::{AttackCatalog, AttackTemplate};

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
        // The Phase-1 templates dir exists in the repo; if this regresses,
        // the dir layout changed and the helper needs updating.
        let dir = templates_dir();
        assert!(
            dir.is_dir(),
            "expected attack templates dir at {}",
            dir.display()
        );
    }

    #[test]
    fn run_list_prints_loaded_attack_templates() {
        // Smoke test: list must not error against the real catalog.
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
}
