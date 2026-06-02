//! Proposed-intent loader for the intent-quality overlay.
//!
//! Reads `<project>/vergil-out/confirm/state.json` (the Phase 6 Stage-2
//! gate's persistence) and returns the multi-oracle stack's proposed
//! intents. Each `ProposedIntent` carries its source oracle
//! (AttackCatalog / Tests / NatSpec / Structural), the intent text the
//! LLM (or template) generated, a confidence score, and a rationale.
//!
//! Returns `Ok(vec![])` if the state file is missing — the sweep may
//! have legitimately produced zero proposed intents on a utility-lib
//! contract where the catalog activates nothing and there are no
//! tests/natspec. That's data, not failure.

use std::path::Path;

pub use vergil_core::confirm::ProposedIntent;
use vergil_core::confirm::{ConfirmState, CONFIRM_STATE_SCHEMA_VERSION};

/// Load proposed intents from `<project>/vergil-out/confirm/state.json`.
///
/// Returns an empty vec if the file is missing (a legitimate
/// utility-lib result, not an error).
pub fn load(project_dir: &Path) -> Result<Vec<ProposedIntent>, String> {
    let state_path = project_dir
        .join("vergil-out")
        .join("confirm")
        .join("state.json");
    if !state_path.is_file() {
        return Ok(vec![]);
    }
    let body = std::fs::read_to_string(&state_path)
        .map_err(|e| format!("read {}: {e}", state_path.display()))?;
    let state: ConfirmState =
        serde_json::from_str(&body).map_err(|e| format!("parse {}: {e}", state_path.display()))?;

    if state.schema_version != CONFIRM_STATE_SCHEMA_VERSION {
        return Err(format!(
            "confirm/state.json schema version mismatch at {}: expected {}, found {}",
            state_path.display(),
            CONFIRM_STATE_SCHEMA_VERSION,
            state.schema_version
        ));
    }

    Ok(state.proposed_intents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::fs;
    use tempfile::TempDir;
    use vergil_core::synthesis::Source;

    fn write_state(td: &TempDir, state: &ConfirmState) {
        let dir = td.path().join("vergil-out").join("confirm");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("state.json"),
            serde_json::to_string_pretty(state).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn returns_empty_when_state_file_missing() {
        let td = TempDir::new().unwrap();
        let v = load(td.path()).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn round_trip_three_proposed_intents() {
        let td = TempDir::new().unwrap();
        let intents = vec![
            ProposedIntent {
                id: "catalog:erc20-no-mint".into(),
                source: Source::AttackCatalog,
                intent_text: "Total supply never increases without onlyOwner.".into(),
                rationale: "Derived from catalog template erc20-no-mint.".into(),
                confidence: 0.92,
                template_ref: Some("erc20-no-mint".into()),
            },
            ProposedIntent {
                id: "tests:check_transfer_conserves_supply".into(),
                source: Source::Tests,
                intent_text: "transfer() preserves totalSupply.".into(),
                rationale: "Test asserts supply unchanged after transfer.".into(),
                confidence: 0.85,
                template_ref: None,
            },
            ProposedIntent {
                id: "structural:invariant-constants:decimals".into(),
                source: Source::Structural,
                intent_text: "decimals is a constant.".into(),
                rationale: "Tier A: uint8 public constant decimals = 18; in source.".into(),
                confidence: 0.95,
                template_ref: Some("structural:invariant-constants:0.95".into()),
            },
        ];
        let state = ConfirmState::new("run-test".into(), intents.clone(), Utc::now());
        write_state(&td, &state);

        let loaded = load(td.path()).unwrap();
        assert_eq!(loaded, intents);
    }

    #[test]
    fn errors_on_schema_version_mismatch() {
        let td = TempDir::new().unwrap();
        let dir = td.path().join("vergil-out").join("confirm");
        fs::create_dir_all(&dir).unwrap();
        let body = r#"{
            "schema_version": 999,
            "run_id": "rid",
            "proposed_intents": [],
            "decisions": [],
            "status": "complete",
            "started_at": "2026-06-02T00:00:00Z",
            "updated_at": "2026-06-02T00:00:00Z"
        }"#;
        fs::write(dir.join("state.json"), body).unwrap();
        let err = load(td.path()).unwrap_err();
        assert!(err.contains("schema version mismatch"));
        assert!(err.contains("999"));
    }

    #[test]
    fn empty_proposed_intents_round_trip() {
        let td = TempDir::new().unwrap();
        let state = ConfirmState::new("rid".into(), vec![], Utc::now());
        write_state(&td, &state);
        let loaded = load(td.path()).unwrap();
        assert!(loaded.is_empty());
    }
}
