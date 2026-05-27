//! Integration test: load the real templates directory committed in this
//! crate. Verifies the reference template parses with both encodings, the
//! corpus meets the Phase 2 30-template target, every license is Apache-2.0
//! per the Tier-2 declaration, and every Halmos encoding has at least one
//! check_ function.

use std::path::PathBuf;

use vergil_properties::{Catalog, CostClass, Tier};

fn templates_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates")
}

#[test]
fn reference_template_loads_with_both_encodings() {
    let cat = Catalog::load(templates_dir()).expect("catalog load");
    let t = cat
        .get("erc20-sum-of-balances")
        .expect("reference template present");
    assert_eq!(t.manifest.cost_class, CostClass::Medium);
    assert_eq!(t.manifest.provenance.tier, Tier::Original);
    assert!(t
        .halmos_source
        .contains("check_transferFrom_preserves_pair_sum"));
    assert!(t.smtchecker_source.contains("_ghostSum == _totalSupply"));
    assert!(t.manifest.requires.storage_slots.len() >= 2);
}

#[test]
fn corpus_meets_phase2_template_count() {
    let cat = Catalog::load(templates_dir()).expect("catalog load");
    assert!(
        cat.len() >= 30,
        "Phase 2 corpus target is 30 templates; loaded {}",
        cat.len()
    );
}

#[test]
fn every_template_is_apache_licensed_and_original() {
    let cat = Catalog::load(templates_dir()).expect("catalog load");
    for t in cat.iter() {
        assert_eq!(
            t.manifest.provenance.tier,
            Tier::Original,
            "{} is not Tier 2 (original); see NOTICE",
            t.manifest.id
        );
        assert!(
            t.manifest
                .provenance
                .license
                .to_ascii_uppercase()
                .starts_with("APACHE-2.0"),
            "{} license is {}, expected Apache-2.0",
            t.manifest.id,
            t.manifest.provenance.license
        );
    }
}

#[test]
fn every_halmos_encoding_has_a_check_function() {
    let cat = Catalog::load(templates_dir()).expect("catalog load");
    for t in cat.iter() {
        assert!(
            t.halmos_source.contains("function check_"),
            "{}: halmos.sol must contain at least one `function check_` symbol",
            t.manifest.id
        );
    }
}
