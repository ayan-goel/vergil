//! Integration test: load the real templates directory committed in this
//! crate and verify the reference template parses cleanly.

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
