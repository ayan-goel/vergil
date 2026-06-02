//! V1.5 Phase 6 Slice 9 — integration tests on the 5-contract reference bed.
//!
//! Two paths exercised. First: `--list-applicable` on every reference
//! contract. Deterministic — no LLM calls. Locks the fingerprint shape
//! (interfaces + primitives + oracles available) and the activation
//! counts (templates + skipped) so a downstream regression to the
//! fingerprint heuristic or activation engine surfaces here.
//!
//! Second: stratified-verdict formatter on hand-built fixtures shaped
//! after each contract's likely Phase-6 outcome. Verifies that the
//! verdict UI renders correctly when the runner's downstream stages
//! (Stage 1-3 LLM + Halmos) produce realistic outputs. Faster + stricter
//! than the live-LLM test because every input is owned.
//!
//! The full live-LLM end-to-end (catalog cex on erc20-broken, headline
//! transitions on the bed) ships in Slice 10 (`phase6_live.rs`) — that
//! test is the SPEC §11.6 exit gate. Slice 9 locks the deterministic
//! seams Slice 10 builds on top.

use std::path::Path;
use std::process::Command;

use vergil_cli::output::verdict::{
    format_verdict, AvailableOraclesSummary, DocumentOnlyTemplate, FingerprintSummary, Headline,
    PropertyOutcome, PropertyVerdict, SkippedTemplateSummary, StratifiedInputs,
};
use vergil_proof::schema::{Source, Tier};

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
}

fn vergil(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO"))
        .args(["run", "-p", "vergil-cli", "--bin", "vergil", "--quiet", "--"])
        .args(args)
        .current_dir(workspace_root())
        .output()
        .expect("cargo run vergil")
}

/// Expected fingerprint + activation shape per reference contract.
/// Locks the Slice 1 detector + Slice 0/3 activation behavior on the
/// 5-contract bed so a regression to either surfaces here.
struct BedContract {
    name: &'static str,
    path: &'static str,
    expected_interfaces: &'static [&'static str],
    expected_primitives: &'static [&'static str],
    has_tests: bool,
    has_natspec: bool,
    /// Minimum template count the activation engine should select.
    /// Catalog grows over time; pin a floor, not an exact count.
    min_activated: usize,
}

const BED: &[BedContract] = &[
    BedContract {
        name: "erc20",
        path: "examples/erc20",
        expected_interfaces: &["ERC20"],
        expected_primitives: &["token-erc20"],
        has_tests: true,
        has_natspec: true,
        min_activated: 10,
    },
    BedContract {
        name: "erc20-broken",
        path: "examples/erc20-broken",
        expected_interfaces: &["ERC20"],
        expected_primitives: &["token-erc20"],
        has_tests: true,
        has_natspec: true,
        min_activated: 10,
    },
    BedContract {
        name: "erc721",
        path: "examples/erc721",
        expected_interfaces: &["ERC721"],
        expected_primitives: &["token-erc721"],
        has_tests: true,
        has_natspec: true,
        min_activated: 10,
    },
    BedContract {
        name: "vault-4626",
        path: "examples/vault-4626",
        expected_interfaces: &["ERC20", "ERC4626"],
        expected_primitives: &["vault"],
        has_tests: true,
        has_natspec: true,
        min_activated: 10,
    },
    BedContract {
        name: "lending",
        path: "examples/lending",
        expected_interfaces: &[],
        expected_primitives: &["lending-market"],
        has_tests: true,
        has_natspec: true,
        min_activated: 10,
    },
];

#[test]
fn list_applicable_matches_expected_shape_on_each_reference_contract() {
    for c in BED {
        let project = workspace_root().join(c.path);
        assert!(
            project.join("foundry.toml").is_file(),
            "{} must be a Foundry project at {}",
            c.name,
            project.display()
        );
        let out = vergil(&[
            "verify",
            project.to_str().expect("utf8 path"),
            "--list-applicable",
        ]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "[{}] --list-applicable should exit 0:\n{stdout}",
            c.name
        );

        // Interfaces line.
        for iface in c.expected_interfaces {
            assert!(
                stdout.contains(iface),
                "[{}] expected interface tag {iface} in list-applicable:\n{stdout}",
                c.name
            );
        }
        // Primitives line.
        for prim in c.expected_primitives {
            assert!(
                stdout.contains(prim),
                "[{}] expected primitive tag {prim} in list-applicable:\n{stdout}",
                c.name
            );
        }
        // Oracle availability.
        assert!(
            stdout.contains(&format!("tests={}", c.has_tests)),
            "[{}] tests oracle availability wrong: {stdout}",
            c.name
        );
        assert!(
            stdout.contains(&format!("natspec={}", c.has_natspec)),
            "[{}] natspec oracle availability wrong: {stdout}",
            c.name
        );

        // Activation count: parse "Activated attack-catalog templates (N):"
        // and assert N >= min_activated.
        let count_line = stdout
            .lines()
            .find(|l| l.contains("Activated attack-catalog templates"))
            .unwrap_or("");
        let parsed_count: usize = count_line
            .split('(')
            .nth(1)
            .and_then(|s| s.split(')').next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        assert!(
            parsed_count >= c.min_activated,
            "[{}] activated only {parsed_count} templates (floor {})",
            c.name,
            c.min_activated,
        );
    }
}

#[test]
fn list_applicable_runs_without_anthropic_key() {
    // The --list-applicable short-circuit MUST exit before provider
    // init — CI and contributors without API keys still get a useful
    // signal. Setting the key to empty mimics the no-key case for
    // belts-and-braces.
    std::env::remove_var("VERGIL_ANTHROPIC_API_KEY");
    std::env::remove_var("ANTHROPIC_API_KEY");
    let project = workspace_root().join("examples/erc20");
    let out = vergil(&[
        "verify",
        project.to_str().expect("utf8 path"),
        "--list-applicable",
    ]);
    assert!(
        out.status.success(),
        "list-applicable without API key should still exit 0:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn catalog_subset_filters_activation_by_category() {
    let project = workspace_root().join("examples/erc20");
    let out = vergil(&[
        "verify",
        project.to_str().expect("utf8 path"),
        "--catalog-subset",
        "reentrancy",
        "--list-applicable",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    // Only reentrancy templates should be in the activated list.
    let activated_block: String = stdout
        .lines()
        .skip_while(|l| !l.contains("Activated attack-catalog templates"))
        .take_while(|l| !l.starts_with("Skipped templates"))
        .collect::<Vec<_>>()
        .join("\n");
    let non_reentrancy_lines: Vec<&str> = activated_block
        .lines()
        .filter(|l| l.trim_start().starts_with("- ") && !l.contains("[reentrancy]"))
        .collect();
    assert!(
        non_reentrancy_lines.is_empty(),
        "--catalog-subset=reentrancy admitted non-reentrancy template:\n{non_reentrancy_lines:?}\nfull stdout:\n{stdout}"
    );
}

// ─── Verdict-formatter snapshots on hand-built fixtures ─────────────────────

/// Erc20-broken-shaped fixture: catalog refutes one property, others
/// verify.
fn fixture_erc20_broken() -> StratifiedInputs {
    StratifiedInputs {
        project_path: "/proj/erc20-broken".to_string(),
        fingerprint: FingerprintSummary {
            interfaces: vec!["ERC20".into()],
            primitives: vec!["token-erc20".into()],
            available_oracles: AvailableOraclesSummary {
                tests: true,
                natspec: true,
                readme: false,
            },
        },
        properties: vec![
            PropertyOutcome {
                name: "check_unauthorized_transferFrom".into(),
                source: Source::AttackCatalog,
                tier: Tier::ZeroConfig,
                verdict: PropertyVerdict::Refuted {
                    backend: "halmos".into(),
                    cex_file: "vergil-out/counterexamples/Cex_check_unauthorized_transferFrom.t.sol"
                        .into(),
                    trace_summary: "attacker transfers tokens they do not own".into(),
                },
                template_ref: Some("access-missing-modifier-state-change".into()),
                intent_text: Some(
                    "Only the token owner can authorize transferFrom on their balance.".into(),
                ),
            },
            PropertyOutcome {
                name: "check_transfer_preserves_supply".into(),
                source: Source::Tests,
                tier: Tier::ZeroConfig,
                verdict: PropertyVerdict::Verified {
                    backend: "halmos".into(),
                    smt_query_sha256: Some("a".repeat(64)),
                },
                template_ref: None,
                intent_text: Some("Transfers preserve totalSupply.".into()),
            },
        ],
        skipped_templates: vec![SkippedTemplateSummary {
            id: "vault-inflation-first-depositor-donation".into(),
            reason: "no overlap with required interfaces [ERC4626]".into(),
        }],
        per_template_failures: vec![],
        document_only_templates: vec![DocumentOnlyTemplate {
            id: "eip7702-delegate-arbitrary-execution".into(),
            name: "EIP-7702 EOA delegate may execute arbitrary code".into(),
        }],
        phase5_structural_pending: true,
        low_confidence_structural: vec![],
    }
}

fn fixture_erc20_clean() -> StratifiedInputs {
    StratifiedInputs {
        project_path: "/proj/erc20".to_string(),
        fingerprint: FingerprintSummary {
            interfaces: vec!["ERC20".into()],
            primitives: vec!["token-erc20".into()],
            available_oracles: AvailableOraclesSummary {
                tests: true,
                natspec: true,
                readme: false,
            },
        },
        properties: vec![
            PropertyOutcome {
                name: "check_supply_preserved".into(),
                source: Source::Tests,
                tier: Tier::ZeroConfig,
                verdict: PropertyVerdict::Verified {
                    backend: "halmos".into(),
                    smt_query_sha256: Some("b".repeat(64)),
                },
                template_ref: None,
                intent_text: Some("Transfers preserve totalSupply.".into()),
            },
            PropertyOutcome {
                name: "check_balance_conservation".into(),
                source: Source::NatSpec,
                tier: Tier::ZeroConfig,
                verdict: PropertyVerdict::Verified {
                    backend: "halmos".into(),
                    smt_query_sha256: Some("c".repeat(64)),
                },
                template_ref: None,
                intent_text: Some("Sum of balances equals totalSupply.".into()),
            },
        ],
        skipped_templates: vec![],
        per_template_failures: vec![],
        document_only_templates: vec![],
        phase5_structural_pending: true,
        low_confidence_structural: vec![],
    }
}

#[test]
fn fixture_erc20_broken_renders_refuted_headline_with_catalog_cex() {
    let v = format_verdict(fixture_erc20_broken());
    assert_eq!(v.headline, Headline::Refuted);
    let report = v.report_md();
    assert!(report.contains("**Headline:** Refuted"));
    assert!(report.contains("source: attack-catalog"));
    assert!(report.contains("Cex_check_unauthorized_transferFrom.t.sol"));
    // Source attribution: catalog refutation must name the template.
    assert!(report.contains("access-missing-modifier-state-change"));
    // Reproduce section is always present and runnable.
    assert!(report.contains("vergil prove /proj/erc20-broken/vergil-out/proof.json"));
    // Not-checked section names Phase 5 deferred + document-only.
    assert!(report.contains("Structural mining (Phase 5)"));
    assert!(report.contains("eip7702-delegate-arbitrary-execution"));
}

#[test]
fn fixture_erc20_clean_renders_verified_in_scope_with_safety_disclaimer() {
    let v = format_verdict(fixture_erc20_clean());
    assert_eq!(v.headline, Headline::VerifiedInScope);
    let report = v.report_md();
    assert!(report.contains("**Headline:** Verified-in-scope"));
    // SPEC §10.3 disclaimer — Verified-in-scope is NOT "safe".
    assert!(report.contains("does NOT mean the contract is safe"));
    // Source attribution on proven properties (tests + natspec).
    assert!(report.contains("source: tests"));
    assert!(report.contains("source: natspec"));
}

#[test]
fn fixture_proof_json_round_trips_after_verdict_format() {
    // The verdict formatter's proof_json output must be valid JSON
    // that downstream consumers (Phase 7 bench, future SaaS UI) can
    // deserialize.
    let v = format_verdict(fixture_erc20_broken());
    let json = v.proof_json();
    let s = serde_json::to_string_pretty(&json).expect("serialize");
    // Top-level keys.
    let parsed: serde_json::Value = serde_json::from_str(&s).expect("re-parse");
    assert_eq!(parsed["headline_machine"], "refuted");
    assert!(parsed["properties"].is_array());
    assert!(parsed["skipped_templates"].is_array());
    assert!(parsed["document_only_templates"].is_array());
    assert!(parsed["reproduce"].as_str().unwrap().contains("vergil prove"));
}

