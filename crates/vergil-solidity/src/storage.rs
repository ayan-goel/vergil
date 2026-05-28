//! Storage-layout wrapper around `solc --combined-json storage-layout`.
//!
//! solc is **authoritative** for storage layout — Slither is intentionally
//! not consulted here. The wrapper invokes solc, parses the JSON it emits,
//! and returns a typed [`StorageLayout`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageResult {
    Ok(Vec<StorageLayout>),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLayout {
    /// `<file path>:<ContractName>` as solc emits it.
    pub qualified_name: String,
    pub entries: Vec<StorageEntry>,
    /// Raw type table from solc (kept for downstream consumers that
    /// need the encoding/numberOfBytes for a slot's type).
    pub types: HashMap<String, StorageType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StorageEntry {
    pub label: String,
    pub slot: String,
    pub offset: u32,
    #[serde(rename = "type")]
    pub type_id: String,
    pub contract: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StorageType {
    pub label: String,
    pub encoding: String,
    #[serde(rename = "numberOfBytes", default)]
    pub number_of_bytes: String,
}

#[derive(Debug, Deserialize)]
struct Combined {
    contracts: HashMap<String, ContractEntry>,
}

#[derive(Debug, Deserialize)]
struct ContractEntry {
    #[serde(rename = "storage-layout")]
    storage_layout: Option<LayoutPayload>,
}

#[derive(Debug, Deserialize)]
struct LayoutPayload {
    #[serde(default)]
    storage: Vec<StorageEntry>,
    #[serde(default)]
    types: HashMap<String, StorageType>,
}

/// Parse `solc --combined-json storage-layout` output.
pub fn parse_combined_json(raw: &str) -> StorageResult {
    let combined: Combined = match serde_json::from_str(raw) {
        Ok(c) => c,
        Err(e) => return StorageResult::Error(format!("invalid solc storage JSON: {e}")),
    };

    let mut out = Vec::new();
    for (qualified, entry) in combined.contracts {
        let payload = match entry.storage_layout {
            Some(p) => p,
            None => continue,
        };
        out.push(StorageLayout {
            qualified_name: qualified,
            entries: payload.storage,
            types: payload.types,
        });
    }
    out.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
    StorageResult::Ok(out)
}

#[derive(Debug, Clone)]
pub struct StorageRun {
    pub source: PathBuf,
    pub wall_clock_budget: Duration,
}

impl StorageRun {
    pub fn new(source: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            wall_clock_budget: Duration::from_secs(30),
        }
    }
}

pub async fn run(cfg: &StorageRun) -> StorageResult {
    if !cfg.source.exists() {
        return StorageResult::Error(format!("source not found: {}", cfg.source.display()));
    }
    let mut cmd = Command::new("solc");
    cmd.arg("--combined-json")
        .arg("storage-layout")
        .arg(&cfg.source)
        .kill_on_drop(true);

    let result = timeout(cfg.wall_clock_budget, cmd.output()).await;
    match result {
        Ok(Ok(output)) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return StorageResult::Error(format!("solc storage layout failed: {stderr}"));
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            parse_combined_json(&stdout)
        }
        Ok(Err(e)) => StorageResult::Error(format!("failed to spawn solc: {e}")),
        Err(_) => StorageResult::Error("solc wall-clock budget exceeded".to_string()),
    }
}

pub async fn run_simple(source: &Path, budget: Duration) -> StorageResult {
    let mut cfg = StorageRun::new(source.to_path_buf());
    cfg.wall_clock_budget = budget;
    run(&cfg).await
}

/// One divergence between two storage layouts. Phase 4 Slice A5: drives
/// the proxy-upgrade stability check. Reported in label-order
/// (alphabetic by old.label, then new.label for purely-added entries).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotDiff {
    /// Slot present in `old` but absent in `new`. Almost always an
    /// upgrade-safety bug: existing storage gets orphaned.
    Removed { label: String, slot: String },
    /// Slot present in `new` but absent in `old`. Safe IF appended at
    /// the end (slot strictly higher than every old slot); dangerous if
    /// inserted in the middle — caller must inspect.
    Added { label: String, slot: String },
    /// Same label, different slot. Almost always a bug — storage at the
    /// old slot is now interpreted as something else by V2.
    SlotMoved {
        label: String,
        old_slot: String,
        new_slot: String,
    },
    /// Same label + slot but different type. Reads/writes diverge silently;
    /// upgrade unsafe.
    TypeChanged {
        label: String,
        slot: String,
        old_type: String,
        new_type: String,
    },
}

/// Diff two storage layouts. An empty result means V1 → V2 is
/// storage-layout-stable (the proxy upgrade is safe). Any entry in the
/// result is something a code reviewer or an automated check should
/// surface.
///
/// Diff semantics:
///   * `Removed` and `SlotMoved` are always bugs.
///   * `TypeChanged` is always a bug.
///   * `Added` is safe **only** if appended at the end. Callers must
///     check that every `Added.slot` is strictly greater than every old
///     slot before treating an upgrade as safe.
pub fn diff_layouts(old: &StorageLayout, new: &StorageLayout) -> Vec<SlotDiff> {
    use std::collections::BTreeMap;
    let old_by_label: BTreeMap<&str, &StorageEntry> =
        old.entries.iter().map(|e| (e.label.as_str(), e)).collect();
    let new_by_label: BTreeMap<&str, &StorageEntry> =
        new.entries.iter().map(|e| (e.label.as_str(), e)).collect();

    let mut out: Vec<SlotDiff> = Vec::new();
    for (label, old_e) in &old_by_label {
        match new_by_label.get(label) {
            None => out.push(SlotDiff::Removed {
                label: (*label).to_string(),
                slot: old_e.slot.clone(),
            }),
            Some(new_e) => {
                if old_e.slot != new_e.slot {
                    out.push(SlotDiff::SlotMoved {
                        label: (*label).to_string(),
                        old_slot: old_e.slot.clone(),
                        new_slot: new_e.slot.clone(),
                    });
                }
                if old_e.type_id != new_e.type_id {
                    out.push(SlotDiff::TypeChanged {
                        label: (*label).to_string(),
                        slot: new_e.slot.clone(),
                        old_type: old_e.type_id.clone(),
                        new_type: new_e.type_id.clone(),
                    });
                }
            }
        }
    }
    for (label, new_e) in &new_by_label {
        if !old_by_label.contains_key(label) {
            out.push(SlotDiff::Added {
                label: (*label).to_string(),
                slot: new_e.slot.clone(),
            });
        }
    }
    out
}

/// Returns `true` when every `Added` entry sits at a slot strictly
/// higher than every entry in `old.entries`. Caller pre-supplies the
/// diff (typically the result of [`diff_layouts`]) so this helper has
/// no extra parse cost.
///
/// Used by the proxy-upgrade flow: appending storage at the end is
/// safe; inserting it in the middle is not.
pub fn additions_are_appended(old: &StorageLayout, diff: &[SlotDiff]) -> bool {
    fn parse_slot(s: &str) -> Option<u128> {
        s.parse::<u128>().ok()
    }
    let max_old = old
        .entries
        .iter()
        .filter_map(|e| parse_slot(&e.slot))
        .max()
        .unwrap_or(0);
    diff.iter().all(|d| match d {
        SlotDiff::Added { slot, .. } => parse_slot(slot).map(|s| s > max_old).unwrap_or(false),
        _ => true, // non-added entries are filtered out by this predicate
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ERC20: &str = include_str!("../tests/fixtures/storage/erc20.json");

    #[test]
    fn erc20_storage_parses() {
        match parse_combined_json(ERC20) {
            StorageResult::Ok(layouts) => {
                assert_eq!(layouts.len(), 1, "expected 1 contract");
                let layout = &layouts[0];
                assert!(layout.qualified_name.ends_with(":Token"));
                assert_eq!(layout.entries.len(), 5);

                let by_label: HashMap<&str, &StorageEntry> = layout
                    .entries
                    .iter()
                    .map(|e| (e.label.as_str(), e))
                    .collect();
                assert_eq!(by_label.get("totalSupply").unwrap().slot, "2");
                assert_eq!(by_label.get("balanceOf").unwrap().slot, "3");
                assert_eq!(by_label.get("allowance").unwrap().slot, "4");

                assert!(layout.types.contains_key("t_uint256"));
            }
            StorageResult::Error(e) => panic!("expected Ok, got Error({e})"),
        }
    }

    #[test]
    fn malformed_json_is_error() {
        assert!(matches!(
            parse_combined_json("xxx"),
            StorageResult::Error(_)
        ));
    }

    fn layout(entries: &[(&str, &str, &str)]) -> StorageLayout {
        StorageLayout {
            qualified_name: "T.sol:T".to_string(),
            entries: entries
                .iter()
                .map(|(label, slot, t)| StorageEntry {
                    label: (*label).to_string(),
                    slot: (*slot).to_string(),
                    offset: 0,
                    type_id: (*t).to_string(),
                    contract: "T.sol:T".to_string(),
                })
                .collect(),
            types: HashMap::new(),
        }
    }

    #[test]
    fn identical_layouts_diff_to_empty() {
        let a = layout(&[("x", "0", "t_uint256"), ("y", "1", "t_address")]);
        let b = layout(&[("x", "0", "t_uint256"), ("y", "1", "t_address")]);
        assert!(diff_layouts(&a, &b).is_empty());
    }

    #[test]
    fn missing_entry_in_new_is_removed() {
        let a = layout(&[("x", "0", "t_uint256"), ("y", "1", "t_address")]);
        let b = layout(&[("x", "0", "t_uint256")]);
        let diffs = diff_layouts(&a, &b);
        assert!(diffs.iter().any(|d| matches!(d, SlotDiff::Removed { label, .. } if label == "y")));
    }

    #[test]
    fn extra_entry_in_new_is_added() {
        let a = layout(&[("x", "0", "t_uint256")]);
        let b = layout(&[("x", "0", "t_uint256"), ("z", "1", "t_address")]);
        let diffs = diff_layouts(&a, &b);
        assert!(diffs.iter().any(|d| matches!(d, SlotDiff::Added { label, .. } if label == "z")));
    }

    #[test]
    fn same_label_different_slot_is_slot_moved() {
        let a = layout(&[("x", "0", "t_uint256"), ("y", "1", "t_address")]);
        let b = layout(&[("x", "1", "t_uint256"), ("y", "0", "t_address")]);
        let diffs = diff_layouts(&a, &b);
        assert_eq!(
            diffs.iter().filter(|d| matches!(d, SlotDiff::SlotMoved { .. })).count(),
            2
        );
    }

    #[test]
    fn type_change_at_same_slot_is_flagged() {
        let a = layout(&[("x", "0", "t_uint256")]);
        let b = layout(&[("x", "0", "t_uint128")]);
        let diffs = diff_layouts(&a, &b);
        assert!(diffs
            .iter()
            .any(|d| matches!(d, SlotDiff::TypeChanged { .. })));
    }

    #[test]
    fn additions_are_appended_passes_for_strictly_higher_slots() {
        let a = layout(&[("x", "0", "t_uint256"), ("y", "1", "t_address")]);
        let b = layout(&[
            ("x", "0", "t_uint256"),
            ("y", "1", "t_address"),
            ("z", "2", "t_bool"),
        ]);
        let diff = diff_layouts(&a, &b);
        assert!(additions_are_appended(&a, &diff));
    }

    #[test]
    fn additions_are_appended_fails_when_added_in_the_middle() {
        // V1 has slots 0+1. V2 has labels x, z, y but z lands at slot 1 —
        // y is "added" relative to V1 but its new slot (2) is fine; the
        // problem is z occupies V1's y slot. This shows up as SlotMoved
        // not Added; additions_are_appended specifically only checks the
        // Added entries. Construct a true "added-in-middle" case:
        let a = layout(&[("x", "0", "t_uint256"), ("y", "2", "t_address")]);
        let b = layout(&[
            ("x", "0", "t_uint256"),
            ("z", "1", "t_bool"),
            ("y", "2", "t_address"),
        ]);
        let diff = diff_layouts(&a, &b);
        // `z` is added at slot 1 < max old slot (2) → should fail.
        assert!(!additions_are_appended(&a, &diff));
    }
}
