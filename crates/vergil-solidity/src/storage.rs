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
}
