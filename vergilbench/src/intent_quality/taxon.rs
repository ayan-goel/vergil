//! Taxonomic classifier for the intent-quality overlay.
//!
//! Maps each English intent string into one of 11 buckets:
//!   - 10 named categories that cover the bench's hand-written intents
//!   - `Other` catch-all for anything that doesn't match
//!
//! The match is "first taxon whose phrase fires wins" — order in the
//! `TAXON_KEYWORDS` table matters. More specific/structural phrases
//! (Reentrancy, Conservation) come before broader ones (Encoding,
//! Configuration) so a multi-flavor intent like "transfers conserve
//! balances and approve sets allowance" classifies as the primary
//! flavor (BalanceConservation) rather than the secondary (Approval).
//!
//! Pure string analysis; no LLM. The keyword sets are calibrated
//! against the 100 hand-written bench intents — Slice 3's acceptance
//! test asserts ≤10/100 fall into `Other`.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IntentTaxon {
    /// "transfers conserve balances", "constant-product invariant",
    /// "deposit/redeem conserve assets and shares".
    BalanceConservation,
    /// "total supply grows by exactly", "burn debits caller balance and
    /// total supply", per-id "minted amount" / "burned amount" patterns.
    SupplyConservation,
    /// "consecutive mints get consecutive ids", "increments by exactly
    /// one", "push then pop round-trips", "setting a bit makes it true".
    Monotonicity,
    /// "only the owner", "only signers", role-gated mint, DEFAULT_ADMIN_ROLE,
    /// transferOwnership / renounceOwnership.
    AccessPolicy,
    /// "transfers revert while paused", "blocked sender cannot transfer",
    /// "funds cannot be released before the deadline", soulbound.
    StateMachineGuard,
    /// "approve sets allowance exactly", "permit nonces start at zero",
    /// "permit past its deadline reverts".
    ApprovalSemantics,
    /// "non-reentrant call to a guarded function succeeds".
    Reentrancy,
    /// "total supply never exceeds the cap", "mint that would breach the
    /// cap reverts", "downcast reverts", "MAX_WALLET".
    BoundsAndOverflow,
    /// "decimals returns 6", "supportsInterface advertises", "reports the
    /// underlying asset", "configured at construction", "default royalty".
    ConfigurationAndIntrospection,
    /// "encoding empty input yields empty string", "digest distinct from
    /// raw hash", "domain separator is non-zero", ECDSA recovery,
    /// "deterministic address from (salt, codeHash, deployer)".
    EncodingAndCrypto,
    /// Catch-all when no keyword fires. Slice 3's acceptance test caps
    /// this at ≤10 of 100 bench ground-truth strings.
    Other,
}

impl fmt::Display for IntentTaxon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

impl IntentTaxon {
    /// Kebab-case wire id. Matches the `#[serde(rename_all)]` shape so
    /// JSON output is consistent.
    pub fn id(&self) -> &'static str {
        match self {
            Self::BalanceConservation => "balance-conservation",
            Self::SupplyConservation => "supply-conservation",
            Self::Monotonicity => "monotonicity",
            Self::AccessPolicy => "access-policy",
            Self::StateMachineGuard => "state-machine-guard",
            Self::ApprovalSemantics => "approval-semantics",
            Self::Reentrancy => "reentrancy",
            Self::BoundsAndOverflow => "bounds-and-overflow",
            Self::ConfigurationAndIntrospection => "configuration-and-introspection",
            Self::EncodingAndCrypto => "encoding-and-crypto",
            Self::Other => "other",
        }
    }
}

/// Order matters: first taxon whose phrase fires wins. Specific /
/// structural categories (Reentrancy, Conservation, Monotonicity) come
/// before broader ones (Encoding, Configuration). A multi-flavor intent
/// classifies as its primary flavor.
///
/// All phrases are matched against the lowercased intent text via
/// `contains` — phrase order within each bucket doesn't matter.
const TAXON_KEYWORDS: &[(IntentTaxon, &[&str])] = &[
    (IntentTaxon::Reentrancy, &["reentrant", "reentrancy"]),
    (
        IntentTaxon::BalanceConservation,
        &[
            "conserve balances",
            "preserve balances",
            "preserve total supply",
            "preserves total supply",
            "preserves the supply",
            "preserve totalassets",
            "conserve assets",
            "conserve shares",
            "transfers preserve",
            "constant-product",
        ],
    ),
    (
        IntentTaxon::SupplyConservation,
        &[
            "supply by exactly",
            "supply grows by",
            "supply reduces",
            "credits",
            "debits",
            "per-id balance",
            "per-id supply",
            "per-id total supply",
            "burn debits",
            "supply increment",
            "minted amount",
            "burned amount",
            "grows total supply",
            "reduces it by",
            "decrements the enumerable total supply",
            "grows enumerable supply",
            "increments enumerable supply",
            "grows by the minted amount",
            "reduces supply",
        ],
    ),
    (
        IntentTaxon::Monotonicity,
        &[
            "consecutive",
            "increments by exactly one",
            "increment the",
            "monotonic",
            "exactly one per mint",
            "by exactly one",
            "round-trips membership",
            "pushing one element",
            "set then get",
            "grows the set by one",
            "grows the map",
            "setting a bit",
            "pushing a (key,value)",
            "incrementing tokenid",
            "auto-increment",
            "consuming one increments",
        ],
    ),
    (
        IntentTaxon::AccessPolicy,
        &[
            "only the owner",
            "only signers",
            "only the forwarder",
            "only undercollateralized",
            "role-gated",
            "role_gated",
            "default_admin_role",
            "default admin",
            "admin_role",
            "admin role",
            "minter role",
            "minter_role",
            "transferownership",
            "renounceownership",
            "owner-gated",
            "non-admin",
            "ownable",
            "accesscontrol",
            "grant roles",
            "borrowing requires",
            "execution requires meeting the confirmation",
            "trusted",
            "operations that were never scheduled",
        ],
    ),
    (
        IntentTaxon::StateMachineGuard,
        &[
            "paused",
            "pausable",
            "unpause",
            "while paused",
            "before the deadline",
            "blocked",
            "blocklist",
            "soulbound",
            "transfer-blocking",
            "timelock",
            "stages a pending owner",
            "until acceptance",
        ],
    ),
    (
        IntentTaxon::ApprovalSemantics,
        &[
            "approve",
            "allowance",
            "approval",
            "permit",
            "nonce",
            "delegation",
            "delegated",
            "self-delegation",
        ],
    ),
    (
        IntentTaxon::BoundsAndOverflow,
        &[
            "cap",
            "max_supply",
            "max_wallet",
            "max-supply",
            "max-wallet",
            "never exceeds",
            "exceeds the",
            "breach",
            "downcast",
            "in-range",
            "out-of-range",
            "headroom",
            "saturat",
            "overflow",
            "max dominates",
            "min is never greater",
        ],
    ),
    (
        IntentTaxon::EncodingAndCrypto,
        &[
            "encode",
            "encoding",
            "digest",
            "hash",
            "domain separator",
            "signature",
            "ecdsa",
            "signed",
            "signer",
            "eip-191",
            "eip-712",
            "round-trip",
            "computeaddress",
            "deterministic",
            "salt",
            "clones",
            "tostring",
            "shortstring",
            "ec recovery",
            "recovers",
        ],
    ),
    (
        IntentTaxon::ConfigurationAndIntrospection,
        &[
            "decimals",
            "interface id",
            "interface ids",
            "supportsinterface",
            "reports the",
            "reports its",
            "reports support",
            "advertises",
            "advertised",
            "configured at construction",
            "default royalty",
            "royalty fraction",
            "receiver",
            "delay configured",
            "cliff",
            "schedule end",
            "constant",
            "underlying",
            "implementation address",
            "proxiableuuid",
            "proxiable uuid",
            "implementation storage slot",
            "vault decimals",
            "default flash fee",
            "maxflashloan",
            "applies both state writes atomically",
            "reads back",
            "no-op",
            "round-trips",
            "exact",
            "exactly",
            "sqrt",
            "muldiv",
            "lt and gt agree",
        ],
    ),
];

/// Classify an intent string into a taxon. Lowercases the input and
/// returns the first taxon whose phrase set fires; `Other` otherwise.
pub fn classify(text: &str) -> IntentTaxon {
    let lower = text.to_lowercase();
    for (taxon, phrases) in TAXON_KEYWORDS {
        for phrase in *phrases {
            if lower.contains(phrase) {
                return *taxon;
            }
        }
    }
    IntentTaxon::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cl(s: &str) -> IntentTaxon {
        classify(s)
    }

    #[test]
    fn balance_conservation() {
        assert_eq!(
            cl("transfers conserve balances and total supply"),
            IntentTaxon::BalanceConservation
        );
        assert_eq!(
            cl("constant-product invariant holds across swaps"),
            IntentTaxon::BalanceConservation
        );
        assert_eq!(
            cl("ERC4626 deposit/redeem conserve assets and shares"),
            IntentTaxon::BalanceConservation
        );
    }

    #[test]
    fn supply_conservation() {
        assert_eq!(
            cl("burn debits caller balance and total supply by exactly the burned amount"),
            IntentTaxon::SupplyConservation
        );
        assert_eq!(
            cl("owner mint grows total supply by the minted amount"),
            IntentTaxon::SupplyConservation
        );
    }

    #[test]
    fn monotonicity() {
        assert_eq!(
            cl("consecutive mints get consecutive ids"),
            IntentTaxon::Monotonicity
        );
        assert_eq!(
            cl("nonces start at zero and consuming one increments the account's nonce by exactly one"),
            IntentTaxon::Monotonicity
        );
    }

    #[test]
    fn access_policy() {
        assert_eq!(
            cl("only the owner may mutate the counter"),
            IntentTaxon::AccessPolicy
        );
        assert_eq!(
            cl("the deployer holds DEFAULT_ADMIN_ROLE"),
            IntentTaxon::AccessPolicy
        );
        assert_eq!(
            cl("only signers can confirm a transaction"),
            IntentTaxon::AccessPolicy
        );
    }

    #[test]
    fn state_machine_guard() {
        assert_eq!(
            cl("transfers revert while paused"),
            IntentTaxon::StateMachineGuard
        );
        assert_eq!(
            cl("funds cannot be released before the deadline"),
            IntentTaxon::StateMachineGuard
        );
    }

    #[test]
    fn approval_semantics() {
        assert_eq!(
            cl("approve sets allowance exactly"),
            IntentTaxon::ApprovalSemantics
        );
        assert_eq!(
            cl("permit nonces start at zero and a permit past its deadline reverts"),
            IntentTaxon::ApprovalSemantics
        );
    }

    #[test]
    fn reentrancy() {
        assert_eq!(
            cl("a normal (non-reentrant) call to a guarded function succeeds"),
            IntentTaxon::Reentrancy
        );
    }

    #[test]
    fn bounds_and_overflow() {
        assert_eq!(
            cl("total supply never exceeds the configured cap"),
            IntentTaxon::BoundsAndOverflow
        );
        assert_eq!(
            cl("an out-of-range downcast reverts"),
            IntentTaxon::BoundsAndOverflow
        );
    }

    #[test]
    fn configuration_and_introspection() {
        assert_eq!(
            cl("decimals() returns 6"),
            IntentTaxon::ConfigurationAndIntrospection
        );
        assert_eq!(
            cl("the contract reports support for the ERC-165 interface id"),
            IntentTaxon::ConfigurationAndIntrospection
        );
    }

    #[test]
    fn encoding_and_crypto() {
        assert_eq!(
            cl("encoding empty input yields an empty string"),
            IntentTaxon::EncodingAndCrypto
        );
        assert_eq!(
            cl("typed-data domain separator is non-zero and stable across calls"),
            IntentTaxon::EncodingAndCrypto
        );
    }

    #[test]
    fn other() {
        assert_eq!(
            cl("this contract does something completely orthogonal"),
            IntentTaxon::Other
        );
    }

    /// Calibration test: classify every ground-truth intent in the
    /// bench corpus and assert ≤10 fall into `Other`. Ignored by
    /// default; run via `cargo test --lib ground_truth_corpus_coverage
    /// -- --ignored --nocapture` when calibrating the keyword sets.
    #[test]
    #[ignore]
    fn ground_truth_corpus_coverage() {
        use crate::intent_quality::ground_truth;
        use std::collections::BTreeMap;

        let corpus = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let entries = ground_truth::load(&corpus).expect("load bench corpus");
        assert_eq!(entries.len(), 100, "expected all 100 bench entries");

        let mut counts: BTreeMap<IntentTaxon, Vec<String>> = BTreeMap::new();
        for (name, gt) in &entries {
            let t = classify(&gt.intent_text);
            counts.entry(t).or_default().push(name.clone());
        }

        eprintln!("\n=== taxon distribution across 100 bench entries ===");
        for (taxon, names) in &counts {
            eprintln!("  {} ({}): {}", taxon, names.len(), names.join(", "));
        }

        let other_count = counts.get(&IntentTaxon::Other).map_or(0, |v| v.len());
        eprintln!("\nOther: {other_count}/100");
        assert!(
            other_count <= 10,
            "expected ≤10 in Other; got {other_count}. \
             Missing taxonomies for: {:?}",
            counts.get(&IntentTaxon::Other)
        );
    }
}
