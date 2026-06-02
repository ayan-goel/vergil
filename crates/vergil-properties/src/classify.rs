//! Primitive classification — V1.5 Phase 3.
//!
//! Replaces the Phase 1 heuristic `vergil_core::fingerprint::detect_primitives`
//! (5 of SPEC §3.3's 13 classes, regex-only, no confidence) with a real
//! classifier combining signature fingerprints, storage-layout
//! fingerprints, inheritance graph, and modifier analysis.
//!
//! ## Output shape
//!
//! [`classify`] returns a [`PrimitiveClassification`] listing every
//! matched primitive with its confidence score and a list of the
//! human-readable signals that drove the match. Catalog activation
//! consumes matches at or above `ClassifyConfig.min_confidence`
//! (default 0.6 per SPEC §3.3 + §10.1's "low-confidence matches
//! surface, never silently act"). Below-threshold matches surface
//! in the verdict's "Suggested classification" section.
//!
//! ## Phase ordering
//!
//! Phase 3 ships in 9 slices. Slice 0 (this file's initial commit)
//! ships the type system + empty stub; Slices 1-4 add detection logic
//! per primitive family; Slices 5-6 ship the ground-truth bench file
//! and regression test (SPEC §11.3 exit gate at ≥90% accuracy); Slice 7
//! wires the classifier into Stage 0 `fingerprint` + the verdict
//! formatter; Slice 8 closes out per CLAUDE.md.

use serde::{Deserialize, Serialize};

use vergil_solidity::storage::StorageLayout;

/// SPEC §3.3 primitive taxonomy. Multi-match is allowed (a contract
/// can carry several tags); per SPEC §10.1 every match carries a
/// confidence and is surfaced or silently consumed based on the
/// configured threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Primitive {
    /// Fungible token implementing the ERC-20 interface.
    TokenErc20,
    /// Non-fungible token implementing the ERC-721 interface.
    TokenErc721,
    /// Multi-token implementing the ERC-1155 interface.
    TokenErc1155,
    /// ERC-4626 or 4626-shaped tokenized vault. Supersedes the
    /// share-token aspect of an ERC-4626 contract: a vault is a vault
    /// first; the share-side ERC-20 tag is carried separately via
    /// `interfaces`.
    Vault,
    /// Lending market with borrow / repay / liquidate surface.
    LendingMarket,
    /// Automated market maker (constant-product or otherwise).
    Amm,
    /// Vesting / release schedule contract.
    Vesting,
    /// Merkle-claim or per-account-mapping airdrop.
    Airdrop,
    /// On-chain governance (Governor-shape or custom propose / vote).
    Governance,
    /// Staking pool with rewards accrual.
    Staking,
    /// Cross-chain bridge endpoint (L1 deposit or L2 withdrawal).
    /// Verification scope per SPEC §3.3 is fingerprint-only.
    Bridge,
    /// Price oracle (Chainlink-shape or push/pull custom).
    Oracle,
    /// Catch-all for contracts with role-based access control but no
    /// other primitive signal — surfaced at low confidence so the
    /// catalog's access-control templates still get a chance to fire.
    AccessControlledGeneric,
}

impl Primitive {
    /// Stable kebab-case identifier. Pinned by SPEC §3.3 taxonomy +
    /// the catalog manifests' `applies_to.primitives` field — do not
    /// rename without updating both.
    pub fn id(self) -> &'static str {
        match self {
            Self::TokenErc20 => "token-erc20",
            Self::TokenErc721 => "token-erc721",
            Self::TokenErc1155 => "token-erc1155",
            Self::Vault => "vault",
            Self::LendingMarket => "lending-market",
            Self::Amm => "amm",
            Self::Vesting => "vesting",
            Self::Airdrop => "airdrop",
            Self::Governance => "governance",
            Self::Staking => "staking",
            Self::Bridge => "bridge",
            Self::Oracle => "oracle",
            Self::AccessControlledGeneric => "access-controlled-generic",
        }
    }

    /// Parse a kebab-case identifier back into a [`Primitive`]. Used by
    /// the ground-truth YAML loader (S5) + telemetry consumers that
    /// see the string form.
    pub fn from_id(s: &str) -> Option<Self> {
        Self::all().into_iter().find(|p| p.id() == s)
    }

    /// All 13 primitives in stable declaration order.
    pub fn all() -> [Primitive; 13] {
        [
            Self::TokenErc20,
            Self::TokenErc721,
            Self::TokenErc1155,
            Self::Vault,
            Self::LendingMarket,
            Self::Amm,
            Self::Vesting,
            Self::Airdrop,
            Self::Governance,
            Self::Staking,
            Self::Bridge,
            Self::Oracle,
            Self::AccessControlledGeneric,
        ]
    }
}

/// One primitive match. Confidence is in `[0.0, 1.0]`; `signals` is
/// the set of human-readable cues that produced the match (e.g.,
/// `["ERC4626 inheritance", "convertToShares + convertToAssets"]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveMatch {
    pub primitive: Primitive,
    /// In `[0.0, 1.0]`. Surfaced as-is; rounded to 2dp by the verdict
    /// formatter.
    pub confidence: f32,
    /// Ordered list of cues that drove the match. Stable per-classifier
    /// so the verdict reads deterministically.
    pub signals: Vec<String>,
}

/// Aggregated output of one classification pass.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveClassification {
    /// All matches, including below-threshold ones. The threshold cut
    /// happens at consumption sites (catalog activation, verdict).
    pub matches: Vec<PrimitiveMatch>,
}

impl PrimitiveClassification {
    /// Top-confidence match (the most likely primitive). `None` when
    /// the classifier had no signals to act on.
    pub fn top(&self) -> Option<&PrimitiveMatch> {
        self.matches
            .iter()
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Iterate matches at or above `threshold`. The catalog activation
    /// engine consumes this iterator (via the verdict runner's
    /// `fingerprint_to_facts`); below-threshold matches go to the
    /// verdict's "Suggested classification" section.
    pub fn above(&self, threshold: f32) -> impl Iterator<Item = &PrimitiveMatch> {
        self.matches.iter().filter(move |m| m.confidence >= threshold)
    }
}

/// Configuration for one classification pass.
#[derive(Debug, Clone)]
pub struct ClassifyConfig {
    /// Cutoff above which matches feed catalog activation. Below this,
    /// matches surface in the verdict but do not drive automatic
    /// activation. Default 0.6 per SPEC §3.3 + §10.1.
    pub min_confidence: f32,
}

impl Default for ClassifyConfig {
    fn default() -> Self {
        Self { min_confidence: 0.6 }
    }
}

/// Phase 3 classifier entry point. Sync + no LLM dependency — pure
/// static analysis (regex over source + solc storage layout).
///
/// `source` is the joined Solidity source text (per-file content
/// concatenated with newline separators — same shape as the Phase 1
/// heuristic consumes).
/// `layouts` is the per-contract solc storage layout, one entry per
/// `<file>:<ContractName>`.
pub fn classify(
    source: &str,
    layouts: &[StorageLayout],
    _cfg: &ClassifyConfig,
) -> PrimitiveClassification {
    let mut matches = Vec::new();
    matches.extend(classify_tokens(source, layouts));
    matches.extend(classify_vault(source, layouts));
    matches.extend(classify_amm(source, layouts));
    matches.extend(classify_lending(source, layouts));
    matches.extend(classify_vesting(source, layouts));
    matches.extend(classify_airdrop(source, layouts));
    matches.extend(classify_governance(source, layouts));
    matches.extend(classify_staking(source, layouts));
    matches.extend(classify_bridge(source, layouts));
    matches.extend(classify_oracle(source, layouts));
    matches.extend(classify_access_controlled_generic(source, &matches));
    PrimitiveClassification { matches }
}

// ─── Classifier 1: Token primitives (ERC20 / ERC721 / ERC1155) ───────

/// Token-primitive classifier. Reuses
/// `vergil_solidity::signatures::detect_interfaces` so the Phase 1 +
/// Phase 6 interface-detection logic stays the source of truth — Phase
/// 3 only re-shapes its output into the [`PrimitiveMatch`] vocabulary.
/// Confidence 0.95 per detected token interface.
///
/// **ERC4626 is intentionally NOT a token primitive** — vault is a
/// vault first (per SPEC §3.3 + Phase 1's `detect_primitives`
/// vault-supersession). The Vault classifier (Slice 2) handles ERC4626
/// contracts; the ERC20 share-token aspect stays in the `interfaces`
/// vec via `detect_interfaces`.
pub fn classify_tokens(source: &str, _layouts: &[StorageLayout]) -> Vec<PrimitiveMatch> {
    use vergil_solidity::signatures::detect_interfaces;
    let mut interface_signals: Vec<&'static str> = Vec::new();
    let mut detected: std::collections::BTreeSet<String> =
        detect_interfaces(source).into_iter().collect();
    if !detected.is_empty() {
        interface_signals.push("detect_interfaces match");
    }

    // Supplemental detection — mirrors Phase 1's `sorted_interfaces` in
    // `vergil_core::fingerprint`. `detect_interfaces` only inspects
    // explicit function declarations; storage-shape patterns like
    // `mapping(address => uint256) public allowance` (which solc
    // auto-getters into a function) are invisible. Surface them here
    // so the classifier matches Phase 1's behavior on the reference
    // contracts.
    let has_public_allowance = source.contains("public allowance");
    let has_public_balanceof = source.contains("public balanceOf")
        || source.contains("public balances")
        || source.contains("balanceOf[");
    let has_function_transfer = source.contains("function transfer(");
    let has_function_transferfrom = source.contains("function transferFrom(");
    let has_erc4626_shape = source.contains("convertToShares")
        || source.contains("convertToAssets")
        || (source.contains("totalAssets") && source.contains("totalShares"));
    let has_erc721_shape = source.contains("ownerOf")
        && (source.contains("safeTransferFrom") || source.contains("setApprovalForAll"));
    let has_erc1155_shape =
        source.contains("safeBatchTransferFrom") && source.contains("balanceOfBatch");

    if has_function_transfer
        && has_function_transferfrom
        && has_public_allowance
        && !has_erc721_shape
    {
        detected.insert("ERC20".to_string());
        interface_signals.push("public allowance + transfer + transferFrom");
    }
    if has_public_balanceof && has_function_transfer && has_public_allowance && !has_erc721_shape {
        detected.insert("ERC20".to_string());
    }
    if has_erc721_shape {
        detected.insert("ERC721".to_string());
    }
    if has_erc1155_shape {
        detected.insert("ERC1155".to_string());
        interface_signals.push("safeBatchTransferFrom + balanceOfBatch");
    }
    if has_erc4626_shape {
        detected.insert("ERC4626".to_string());
    }

    let mut out = Vec::new();
    // ERC4626 is intentionally NOT a token primitive — vault is a
    // vault first (per SPEC §3.3 + Phase 1's `detect_primitives`
    // vault-supersession). Suppress the token match so multi-match
    // doesn't double-bill an ERC-4626 vault as both vault AND
    // token-erc20. The Vault classifier (Slice 2) owns this contract.
    if detected.contains("ERC4626") {
        return out;
    }
    let extra: Vec<String> = interface_signals.iter().map(|s| s.to_string()).collect();
    if detected.contains("ERC20") {
        let mut signals = vec!["ERC20 interface".to_string()];
        signals.extend(extra.iter().cloned());
        out.push(PrimitiveMatch {
            primitive: Primitive::TokenErc20,
            confidence: 0.95,
            signals,
        });
    }
    if detected.contains("ERC721") {
        out.push(PrimitiveMatch {
            primitive: Primitive::TokenErc721,
            confidence: 0.95,
            signals: vec!["ERC721 interface (ownerOf + approval surface)".into()],
        });
    }
    if detected.contains("ERC1155") {
        out.push(PrimitiveMatch {
            primitive: Primitive::TokenErc1155,
            confidence: 0.95,
            signals: vec!["safeBatchTransferFrom + balanceOfBatch present".into()],
        });
    }
    out
}

// ─── Classifier 2: Vault (ERC-4626) ──────────────────────────────────

/// Vault classifier — multi-signal detection for ERC-4626-shaped
/// contracts. Five recognized signals:
///
/// 1. **Inheritance**: `is ERC4626` / `extends ERC4626` (real
///    derived-contract pattern).
/// 2. **Contract name**: `contract ERC4626 {` (direct-implementation
///    pattern, used by `examples/vault-4626`).
/// 3. **Convert pair**: `convertToShares + convertToAssets` both
///    present (the ERC-4626 spec's distinguishing accounting surface).
/// 4. **`totalAssets()` getter**: explicit declaration of the function
///    or a `function totalAssets()` signature.
/// 5. **`asset()` getter**: explicit `function asset()` declaration.
///
/// 2+ signals → confidence 0.95. 1 signal → 0.70. Per SPEC §3.3, vault
/// supersedes the share-token aspect of an ERC-4626 contract; the
/// token classifier (Slice 1) suppresses ERC-4626 explicitly.
pub fn classify_vault(source: &str, _layouts: &[StorageLayout]) -> Vec<PrimitiveMatch> {
    let mut signals: Vec<String> = Vec::new();

    // Signal 1: inheritance.
    if has_inheritance_of(source, "ERC4626") {
        signals.push("inherits ERC4626".into());
    }
    // Signal 2: contract-name match (direct implementation).
    if source.contains("contract ERC4626") || source.contains("contract Vault") {
        signals.push("contract named ERC4626 / Vault".into());
    }
    // Signal 3: convert pair.
    if source.contains("convertToShares") && source.contains("convertToAssets") {
        signals.push("convertToShares + convertToAssets".into());
    }
    // Signal 4: totalAssets getter.
    if source.contains("function totalAssets") || source.contains("totalAssets()") {
        signals.push("totalAssets() surface".into());
    }
    // Signal 5: asset() getter.
    if source.contains("function asset(") {
        signals.push("asset() getter".into());
    }

    if signals.is_empty() {
        return Vec::new();
    }
    let confidence = if signals.len() >= 2 { 0.95 } else { 0.70 };
    vec![PrimitiveMatch {
        primitive: Primitive::Vault,
        confidence,
        signals,
    }]
}

// ─── Classifier 3: AMM ───────────────────────────────────────────────

/// AMM (automated market maker) classifier. Three signals:
///
/// 1. **Swap surface**: any `function swap*` (covers `swap`,
///    `swapXForY`, `swapExactTokensForTokens`, etc.).
/// 2. **Reserves storage**: state vars matching `reserve[01XY]?` /
///    `reserves[01]?` — the constant-product AMM canonical shape.
/// 3. **Contract name**: `contract AMM` or `contract <name>Pair`.
///
/// 2+ signals → 0.90. 1 signal → 0.65 (swap alone is too common to
/// activate on).
pub fn classify_amm(source: &str, _layouts: &[StorageLayout]) -> Vec<PrimitiveMatch> {
    let mut signals: Vec<String> = Vec::new();
    if has_function_starting_with(source, "swap") {
        signals.push("swap* function present".into());
    }
    if has_reserves_storage(source) {
        signals.push("reserves storage (reserve0/1 or reserveX/Y)".into());
    }
    if source.contains("contract AMM") || contains_pair_contract(source) {
        signals.push("AMM / Pair contract name".into());
    }
    if signals.is_empty() {
        return Vec::new();
    }
    let confidence = if signals.len() >= 2 { 0.90 } else { 0.65 };
    vec![PrimitiveMatch {
        primitive: Primitive::Amm,
        confidence,
        signals,
    }]
}

fn has_function_starting_with(source: &str, prefix: &str) -> bool {
    let needle = format!("function {prefix}");
    source.contains(&needle)
}

fn has_reserves_storage(source: &str) -> bool {
    // Look for state-var declarations with the canonical names.
    for name in [
        "reserve0", "reserve1", "reserveX", "reserveY", "reserves0", "reserves1",
    ] {
        // Quick check: any uint declaration referencing the name.
        if source.contains(name) {
            // Ensure it's a state-var-ish context (uint or public).
            let near_uint = source.contains(&format!("uint256 public {name}"))
                || source.contains(&format!("uint256 {name}"))
                || source.contains(&format!("uint128 public {name}"))
                || source.contains(&format!("uint128 {name}"))
                || source.contains(&format!("public {name}"));
            if near_uint {
                return true;
            }
        }
    }
    false
}

fn contains_pair_contract(source: &str) -> bool {
    // `contract FooPair {` — common Uniswap-V2 derived-pair pattern.
    let mut i = 0;
    while let Some(rel) = source[i..].find("contract ") {
        let start = i + rel + "contract ".len();
        let rest = &source[start..];
        let end = rest.find(|c: char| c == ' ' || c == '{').unwrap_or(rest.len());
        let name = &rest[..end];
        if name.ends_with("Pair") && name.len() > "Pair".len() {
            return true;
        }
        i = start;
    }
    false
}

// ─── Classifier 4: Lending market ────────────────────────────────────

/// Lending-market classifier. Three canonical signals:
///
/// - `function borrow` present
/// - `function repay` present
/// - `function liquidate` present
///
/// All three → 0.90. Two of three → 0.65.
pub fn classify_lending(source: &str, _layouts: &[StorageLayout]) -> Vec<PrimitiveMatch> {
    let mut signals: Vec<String> = Vec::new();
    if has_function_starting_with(source, "borrow") {
        signals.push("borrow() surface".into());
    }
    if has_function_starting_with(source, "repay") {
        signals.push("repay() surface".into());
    }
    if has_function_starting_with(source, "liquidate") {
        signals.push("liquidate() surface".into());
    }
    if signals.is_empty() {
        return Vec::new();
    }
    let confidence = if signals.len() >= 3 {
        0.90
    } else if signals.len() == 2 {
        0.65
    } else {
        // A lone `borrow()` is too noisy — many non-lending contracts
        // borrow flash loans. Don't emit at all.
        return Vec::new();
    };
    vec![PrimitiveMatch {
        primitive: Primitive::LendingMarket,
        confidence,
        signals,
    }]
}

// ─── Classifier 5: Long-tail primitives ──────────────────────────────

/// Vesting classifier. Signals: `release()` + (`beneficiary()` or
/// `releaseTime`). Confidence 0.75 for the signal pair; below
/// threshold otherwise.
pub fn classify_vesting(source: &str, _layouts: &[StorageLayout]) -> Vec<PrimitiveMatch> {
    let mut signals: Vec<String> = Vec::new();
    if has_function_starting_with(source, "release") {
        signals.push("release() surface".into());
    }
    let beneficiary_signal = source.contains("beneficiary()")
        || source.contains("function beneficiary")
        || source.contains(" beneficiary")
        || source.contains("releaseTime")
        || source.contains("releasable")
        || source.contains("vestingSchedule");
    if beneficiary_signal {
        signals.push("vesting state (beneficiary / releaseTime / releasable)".into());
    }
    if signals.len() < 2 {
        return Vec::new();
    }
    vec![PrimitiveMatch {
        primitive: Primitive::Vesting,
        confidence: 0.75,
        signals,
    }]
}

/// Airdrop classifier. Signals: `claim()` + (`merkleRoot` storage or
/// `claimed[address]` mapping). Confidence 0.75.
pub fn classify_airdrop(source: &str, _layouts: &[StorageLayout]) -> Vec<PrimitiveMatch> {
    let mut signals: Vec<String> = Vec::new();
    if has_function_starting_with(source, "claim") {
        signals.push("claim() surface".into());
    }
    if source.contains("merkleRoot")
        || source.contains("merkle_root")
        || source.contains("MerkleProof")
    {
        signals.push("merkleRoot / MerkleProof".into());
    } else if source.contains("claimed[") || source.contains("hasClaimed[") {
        signals.push("claimed[address] mapping".into());
    }
    if signals.len() < 2 {
        return Vec::new();
    }
    vec![PrimitiveMatch {
        primitive: Primitive::Airdrop,
        confidence: 0.75,
        signals,
    }]
}

/// Governance classifier. Signals: `propose() + (queue|execute) +
/// castVote` OR `Governor` inheritance. Confidence 0.75.
pub fn classify_governance(source: &str, _layouts: &[StorageLayout]) -> Vec<PrimitiveMatch> {
    let mut signals: Vec<String> = Vec::new();
    if has_inheritance_of(source, "Governor") || has_inheritance_of(source, "GovernorBravo") {
        signals.push("inherits Governor".into());
    }
    let propose = has_function_starting_with(source, "propose");
    let cast_vote = source.contains("castVote") || source.contains("function vote(");
    let queue_or_exec =
        has_function_starting_with(source, "queue") || has_function_starting_with(source, "execute");
    if propose {
        signals.push("propose() surface".into());
    }
    if cast_vote {
        signals.push("castVote / vote() surface".into());
    }
    if queue_or_exec {
        signals.push("queue() / execute() surface".into());
    }
    if signals.len() < 2 {
        return Vec::new();
    }
    vec![PrimitiveMatch {
        primitive: Primitive::Governance,
        confidence: 0.75,
        signals,
    }]
}

/// Staking classifier. Signals: `stake()` + (`unstake` or `withdraw`)
/// + `rewards` (storage or function name). Confidence 0.75.
pub fn classify_staking(source: &str, _layouts: &[StorageLayout]) -> Vec<PrimitiveMatch> {
    let mut signals: Vec<String> = Vec::new();
    if has_function_starting_with(source, "stake") {
        signals.push("stake() surface".into());
    }
    if has_function_starting_with(source, "unstake")
        || source.contains("function withdraw(uint256")
    {
        signals.push("unstake() / withdraw() surface".into());
    }
    if source.contains("rewards")
        || source.contains("Rewards")
        || source.contains("rewardPerToken")
    {
        signals.push("rewards state / function".into());
    }
    if signals.len() < 2 {
        return Vec::new();
    }
    vec![PrimitiveMatch {
        primitive: Primitive::Staking,
        confidence: 0.75,
        signals,
    }]
}

/// Bridge classifier. Signals: `deposit + finalizeDeposit` (L1) OR
/// `claimWithdrawal` (L2). Confidence 0.75.
pub fn classify_bridge(source: &str, _layouts: &[StorageLayout]) -> Vec<PrimitiveMatch> {
    let mut signals: Vec<String> = Vec::new();
    let deposit = has_function_starting_with(source, "deposit");
    let finalize = source.contains("finalizeDeposit") || source.contains("finalizeWithdrawal");
    let claim_w = source.contains("claimWithdrawal") || source.contains("proveWithdrawal");
    if deposit && finalize {
        signals.push("deposit + finalize* (L1 bridge)".into());
    }
    if claim_w {
        signals.push("claimWithdrawal / proveWithdrawal (L2 bridge)".into());
    }
    if signals.is_empty() {
        return Vec::new();
    }
    vec![PrimitiveMatch {
        primitive: Primitive::Bridge,
        confidence: 0.75,
        signals,
    }]
}

/// Oracle classifier. Signals: `latestAnswer() + decimals()`
/// (Chainlink shape) OR `getPrice() + update()`. Confidence 0.75.
pub fn classify_oracle(source: &str, _layouts: &[StorageLayout]) -> Vec<PrimitiveMatch> {
    let mut signals: Vec<String> = Vec::new();
    let chainlink = source.contains("latestAnswer") && source.contains("decimals()");
    let custom = (source.contains("getPrice") || source.contains("function price"))
        && (has_function_starting_with(source, "update") || source.contains("postPrice"));
    if chainlink {
        signals.push("latestAnswer() + decimals() (Chainlink shape)".into());
    }
    if custom {
        signals.push("getPrice() + update() (custom oracle)".into());
    }
    if signals.is_empty() {
        return Vec::new();
    }
    vec![PrimitiveMatch {
        primitive: Primitive::Oracle,
        confidence: 0.75,
        signals,
    }]
}

/// Access-controlled-generic catch-all. Fires when **no other primitive
/// landed at confidence ≥0.6** AND a role-based modifier is present.
/// Confidence 0.65 — surfaced but right at the activation boundary so
/// it doesn't paper over a missed real-primitive classification.
///
/// The `existing` slice carries the matches the other classifiers
/// produced so the catch-all can defer when a real primitive owns
/// the contract.
pub fn classify_access_controlled_generic(
    source: &str,
    existing: &[PrimitiveMatch],
) -> Vec<PrimitiveMatch> {
    // If any existing match is at or above the default threshold,
    // defer — the contract already has a primary primitive.
    if existing.iter().any(|m| m.confidence >= 0.6) {
        return Vec::new();
    }
    let mut signals: Vec<String> = Vec::new();
    if source.contains("onlyOwner") || source.contains("modifier onlyOwner") {
        signals.push("onlyOwner modifier".into());
    }
    if source.contains("onlyRole(") {
        signals.push("onlyRole(...) modifier".into());
    }
    if source.contains("hasRole(") {
        signals.push("hasRole(...) check".into());
    }
    if source.contains("AccessControl") {
        signals.push("AccessControl reference".into());
    }
    if signals.is_empty() {
        return Vec::new();
    }
    vec![PrimitiveMatch {
        primitive: Primitive::AccessControlledGeneric,
        confidence: 0.65,
        signals,
    }]
}

/// Detect whether the contract declares inheritance from `parent`.
/// Matches both `is ParentA, Parent,` and `is Parent {` styles.
fn has_inheritance_of(source: &str, parent: &str) -> bool {
    // Look for `is ... <parent> ...` on lines with a `contract` decl.
    // The cheap-and-cheerful check: any `is ` followed eventually by
    // the parent identifier within ~200 chars.
    let mut i = 0;
    while let Some(rel) = source[i..].find("contract ") {
        let start = i + rel + "contract ".len();
        let end = source.len().min(start + 300);
        let window = &source[start..end];
        if window.contains(parent) {
            // Confirm the `parent` ident appears AFTER an `is` keyword
            // in the inheritance list (not as a substring of the
            // contract name).
            if let Some(is_idx) = window.find(" is ") {
                let tail = &window[is_idx + 4..];
                let tail_end = tail.find('{').unwrap_or(tail.len());
                if tail[..tail_end].contains(parent) {
                    return true;
                }
            }
        }
        i = start;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_ids_are_stable_kebab_case() {
        assert_eq!(Primitive::TokenErc20.id(), "token-erc20");
        assert_eq!(Primitive::TokenErc721.id(), "token-erc721");
        assert_eq!(Primitive::TokenErc1155.id(), "token-erc1155");
        assert_eq!(Primitive::Vault.id(), "vault");
        assert_eq!(Primitive::LendingMarket.id(), "lending-market");
        assert_eq!(Primitive::Amm.id(), "amm");
        assert_eq!(Primitive::Vesting.id(), "vesting");
        assert_eq!(Primitive::Airdrop.id(), "airdrop");
        assert_eq!(Primitive::Governance.id(), "governance");
        assert_eq!(Primitive::Staking.id(), "staking");
        assert_eq!(Primitive::Bridge.id(), "bridge");
        assert_eq!(Primitive::Oracle.id(), "oracle");
        assert_eq!(
            Primitive::AccessControlledGeneric.id(),
            "access-controlled-generic"
        );
    }

    #[test]
    fn primitive_all_has_all_thirteen() {
        let v = Primitive::all();
        assert_eq!(v.len(), 13);
        // First / last anchor the order (catalog activation engines
        // may pin on declaration order for stable display).
        assert_eq!(v[0], Primitive::TokenErc20);
        assert_eq!(v[12], Primitive::AccessControlledGeneric);
    }

    #[test]
    fn primitive_from_id_round_trips() {
        for p in Primitive::all() {
            assert_eq!(Primitive::from_id(p.id()), Some(p), "round-trip for {:?}", p);
        }
        assert_eq!(Primitive::from_id("not-a-primitive"), None);
        assert_eq!(Primitive::from_id(""), None);
    }

    #[test]
    fn empty_classify_returns_no_matches() {
        let cfg = ClassifyConfig::default();
        let c = classify("", &[], &cfg);
        assert!(c.matches.is_empty());
        assert!(c.top().is_none());
        assert_eq!(c.above(0.6).count(), 0);
    }

    #[test]
    fn config_default_threshold_is_06() {
        let cfg = ClassifyConfig::default();
        assert!((cfg.min_confidence - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn top_picks_highest_confidence_match() {
        let c = PrimitiveClassification {
            matches: vec![
                PrimitiveMatch {
                    primitive: Primitive::AccessControlledGeneric,
                    confidence: 0.65,
                    signals: vec!["onlyOwner".into()],
                },
                PrimitiveMatch {
                    primitive: Primitive::TokenErc20,
                    confidence: 0.95,
                    signals: vec!["ERC20 interface".into()],
                },
                PrimitiveMatch {
                    primitive: Primitive::Amm,
                    confidence: 0.5,
                    signals: vec!["swap()".into()],
                },
            ],
        };
        assert_eq!(c.top().map(|m| m.primitive), Some(Primitive::TokenErc20));
        let above: Vec<_> = c.above(0.6).map(|m| m.primitive).collect();
        assert!(above.contains(&Primitive::TokenErc20));
        assert!(above.contains(&Primitive::AccessControlledGeneric));
        assert!(!above.contains(&Primitive::Amm), "0.5 must be excluded at threshold 0.6");
    }

    // ─── Token classifiers (S1) ──────────────────────────────────────

    fn fixture(name: &str) -> String {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/classify")
            .join(name);
        std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
    }

    fn examples_root() -> std::path::PathBuf {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop(); // crates/vergil-properties
        p.pop(); // crates
        p.push("examples");
        p
    }

    fn example_source(relative_path: &str) -> String {
        let p = examples_root().join(relative_path);
        std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
    }

    #[test]
    fn classify_tokens_erc20_emits_high_confidence_match() {
        let src = example_source("erc20/src/Token.sol");
        let matches = classify_tokens(&src, &[]);
        let erc20 = matches.iter().find(|m| m.primitive == Primitive::TokenErc20);
        assert!(erc20.is_some(), "expected TokenErc20 match: {matches:#?}");
        let m = erc20.unwrap();
        assert!((m.confidence - 0.95).abs() < 1e-3);
        assert!(m.signals[0].contains("ERC20"), "signal: {}", m.signals[0]);
        // Must NOT carry ERC721 / ERC1155.
        assert!(matches
            .iter()
            .all(|m| m.primitive != Primitive::TokenErc721
                && m.primitive != Primitive::TokenErc1155));
    }

    #[test]
    fn classify_tokens_erc721_emits_match_and_not_erc20() {
        let src = example_source("erc721/src/ERC721.sol");
        let matches = classify_tokens(&src, &[]);
        assert!(matches.iter().any(|m| m.primitive == Primitive::TokenErc721));
        // The Phase 1 stragglers' root cause: ERC721 must NOT also
        // carry an ERC20 token primitive. Pinned by SPEC §3.3 and
        // notes/phase4-a1-stragglers-diagnosis.md.
        assert!(
            !matches.iter().any(|m| m.primitive == Primitive::TokenErc20),
            "ERC721 leaked TokenErc20: {matches:#?}"
        );
    }

    #[test]
    fn classify_tokens_erc1155_emits_via_batch_signatures() {
        let src = fixture("erc1155_minimal.sol");
        let matches = classify_tokens(&src, &[]);
        let erc1155 = matches.iter().find(|m| m.primitive == Primitive::TokenErc1155);
        assert!(
            erc1155.is_some(),
            "expected TokenErc1155 match on safeBatchTransferFrom + balanceOfBatch: {matches:#?}"
        );
        assert!((erc1155.unwrap().confidence - 0.95).abs() < 1e-3);
    }

    #[test]
    fn classify_tokens_utility_lib_emits_no_match() {
        let src = fixture("utility_lib.sol");
        let matches = classify_tokens(&src, &[]);
        assert!(
            matches.is_empty(),
            "utility lib should not classify as a token: {matches:#?}"
        );
    }

    #[test]
    fn classify_tokens_erc4626_suppresses_token_match() {
        // Vault classifier (Slice 2) owns ERC-4626; token classifier
        // must NOT double-bill a vault as TokenErc20.
        let src = example_source("vault-4626/src/ERC4626.sol");
        let matches = classify_tokens(&src, &[]);
        assert!(
            matches.is_empty(),
            "ERC4626 contract must not produce token primitives: {matches:#?}"
        );
    }

    #[test]
    fn classify_returns_token_matches_via_aggregator() {
        // End-to-end through `classify()` confirms the aggregator wires
        // the token classifier into the public output.
        let src = example_source("erc20/src/Token.sol");
        let cfg = ClassifyConfig::default();
        let report = classify(&src, &[], &cfg);
        assert_eq!(report.top().map(|m| m.primitive), Some(Primitive::TokenErc20));
        let above: Vec<_> = report.above(0.6).map(|m| m.primitive).collect();
        assert!(above.contains(&Primitive::TokenErc20));
    }

    // ─── Vault classifier (S2) ───────────────────────────────────────

    #[test]
    fn classify_vault_high_confidence_on_examples_vault_4626() {
        let src = example_source("vault-4626/src/ERC4626.sol");
        let matches = classify_vault(&src, &[]);
        let vault = matches
            .iter()
            .find(|m| m.primitive == Primitive::Vault)
            .expect("expected Vault match");
        assert!(
            (vault.confidence - 0.95).abs() < 1e-3,
            "expected 0.95, got {}",
            vault.confidence
        );
        // Multi-signal: contract-name + convert-pair + totalAssets at least.
        assert!(
            vault.signals.len() >= 2,
            "expected 2+ signals: {:#?}",
            vault.signals
        );
    }

    #[test]
    fn classify_vault_low_confidence_on_single_signal() {
        let src = fixture("vault_single_signal.sol");
        let matches = classify_vault(&src, &[]);
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.primitive, Primitive::Vault);
        assert!((m.confidence - 0.70).abs() < 1e-3, "expected 0.70, got {}", m.confidence);
        assert_eq!(m.signals.len(), 1);
    }

    #[test]
    fn classify_vault_no_match_on_erc20() {
        let src = example_source("erc20/src/Token.sol");
        let matches = classify_vault(&src, &[]);
        assert!(matches.is_empty(), "ERC20 should not classify as vault: {matches:#?}");
    }

    #[test]
    fn classify_returns_only_vault_on_erc4626() {
        // End-to-end: an ERC-4626 contract should produce a Vault
        // match but NO TokenErc20 match (vault-supersession).
        let src = example_source("vault-4626/src/ERC4626.sol");
        let report = classify(&src, &[], &ClassifyConfig::default());
        let top = report.top().expect("expected top match");
        assert_eq!(top.primitive, Primitive::Vault);
        assert!(report
            .matches
            .iter()
            .all(|m| m.primitive != Primitive::TokenErc20));
    }

    #[test]
    fn has_inheritance_detects_derived_contracts() {
        let src = r#"
            contract Strategy is ERC4626, Ownable {}
        "#;
        assert!(has_inheritance_of(src, "ERC4626"));
        assert!(has_inheritance_of(src, "Ownable"));
        assert!(!has_inheritance_of(src, "ERC20"));
    }

    #[test]
    fn has_inheritance_does_not_match_substrings() {
        // `MyERC4626` is a contract name, not a parent.
        let src = "contract MyERC4626 { }";
        assert!(!has_inheritance_of(src, "ERC4626"));
    }

    // ─── AMM + lending classifiers (S3) ──────────────────────────────

    #[test]
    fn classify_amm_high_confidence_on_examples_amm() {
        let src = example_source("amm-constant-product/src/AMM.sol");
        let matches = classify_amm(&src, &[]);
        let amm = matches.iter().find(|m| m.primitive == Primitive::Amm).expect("amm");
        assert!((amm.confidence - 0.90).abs() < 1e-3, "expected 0.90, got {}", amm.confidence);
        assert!(amm.signals.len() >= 2, "expected 2+ signals: {:#?}", amm.signals);
    }

    #[test]
    fn classify_amm_low_confidence_on_single_signal() {
        // A contract with `function swap()` only — no reserves, no Pair
        // name — should land at 0.65 (below threshold; surfaced only).
        let src = "contract X { function swap(uint256 x) external returns (uint256) { return x; } }";
        let matches = classify_amm(src, &[]);
        assert_eq!(matches.len(), 1);
        assert!((matches[0].confidence - 0.65).abs() < 1e-3);
    }

    #[test]
    fn classify_amm_no_match_on_erc20() {
        let src = example_source("erc20/src/Token.sol");
        let matches = classify_amm(&src, &[]);
        assert!(matches.is_empty(), "ERC20 should not classify as AMM: {matches:#?}");
    }

    #[test]
    fn classify_lending_high_confidence_on_examples_lending() {
        let src = example_source("lending/src/Lending.sol");
        let matches = classify_lending(&src, &[]);
        let lend = matches
            .iter()
            .find(|m| m.primitive == Primitive::LendingMarket)
            .expect("lending");
        assert!((lend.confidence - 0.90).abs() < 1e-3, "got {}", lend.confidence);
        assert_eq!(lend.signals.len(), 3);
    }

    #[test]
    fn classify_lending_partial_match_at_065() {
        let src = "contract L { function borrow(uint256 n) external {} function repay(uint256 n) external {} }";
        let matches = classify_lending(src, &[]);
        assert_eq!(matches.len(), 1);
        assert!((matches[0].confidence - 0.65).abs() < 1e-3);
    }

    #[test]
    fn classify_lending_single_signal_drops_to_zero() {
        // Lone `borrow()` is too noisy (flash-loan callers); don't emit.
        let src = "contract X { function borrow(uint256 n) external {} }";
        let matches = classify_lending(src, &[]);
        assert!(matches.is_empty(), "expected no lending match: {matches:#?}");
    }

    #[test]
    fn classify_lending_no_match_on_erc20() {
        let src = example_source("erc20/src/Token.sol");
        let matches = classify_lending(&src, &[]);
        assert!(matches.is_empty(), "ERC20 should not classify as lending: {matches:#?}");
    }

    // ─── Long-tail classifiers (S4) ──────────────────────────────────

    #[test]
    fn classify_vesting_emits_on_fixture() {
        let src = fixture("vesting.sol");
        let matches = classify_vesting(&src, &[]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].primitive, Primitive::Vesting);
        assert!((matches[0].confidence - 0.75).abs() < 1e-3);
    }

    #[test]
    fn classify_vesting_no_match_on_erc20() {
        let src = example_source("erc20/src/Token.sol");
        assert!(classify_vesting(&src, &[]).is_empty());
    }

    #[test]
    fn classify_airdrop_emits_on_fixture() {
        let src = fixture("airdrop.sol");
        let matches = classify_airdrop(&src, &[]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].primitive, Primitive::Airdrop);
    }

    #[test]
    fn classify_governance_emits_on_fixture() {
        let src = fixture("governance.sol");
        let matches = classify_governance(&src, &[]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].primitive, Primitive::Governance);
        assert!(matches[0].signals.len() >= 2);
    }

    #[test]
    fn classify_staking_emits_on_fixture() {
        let src = fixture("staking.sol");
        let matches = classify_staking(&src, &[]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].primitive, Primitive::Staking);
        assert!(matches[0].signals.len() >= 3);
    }

    #[test]
    fn classify_bridge_emits_on_fixture() {
        let src = fixture("bridge.sol");
        let matches = classify_bridge(&src, &[]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].primitive, Primitive::Bridge);
    }

    #[test]
    fn classify_oracle_emits_on_fixture() {
        let src = fixture("oracle.sol");
        let matches = classify_oracle(&src, &[]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].primitive, Primitive::Oracle);
    }

    #[test]
    fn classify_access_controlled_fires_when_nothing_else_matched() {
        let src = fixture("access_controlled.sol");
        // Run via the full aggregator to confirm the catch-all only
        // fires when no other primitive landed.
        let report = classify(&src, &[], &ClassifyConfig::default());
        let top = report.top().expect("expected top match");
        assert_eq!(top.primitive, Primitive::AccessControlledGeneric);
        assert!((top.confidence - 0.65).abs() < 1e-3);
    }

    #[test]
    fn classify_access_controlled_defers_when_real_primitive_matched() {
        // An ERC20 with onlyOwner — TokenErc20 wins; AccessControlledGeneric
        // must NOT fire.
        let src = example_source("erc20/src/Token.sol");
        let report = classify(&src, &[], &ClassifyConfig::default());
        assert!(report
            .matches
            .iter()
            .all(|m| m.primitive != Primitive::AccessControlledGeneric));
    }

    #[test]
    fn long_tail_single_signal_drops_below_threshold() {
        // `function release()` alone (no beneficiary / releaseTime) →
        // no vesting match.
        let src = "contract X { function release() external {} }";
        assert!(classify_vesting(src, &[]).is_empty());
    }

    #[test]
    fn serde_round_trip_keeps_kebab_case() {
        // Wire format is pinned by SPEC §3.3 + catalog manifests'
        // applies_to.primitives field; do not let it drift.
        let m = PrimitiveMatch {
            primitive: Primitive::Vault,
            confidence: 0.95,
            signals: vec!["ERC4626 inheritance".into()],
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("\"primitive\":\"vault\""), "{s}");
        let back: PrimitiveMatch = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
    }
}
