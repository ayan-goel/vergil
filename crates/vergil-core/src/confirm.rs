//! Stage 2 — Intent confirmation gate. V1.5 Phase 6 Slice 7 / SPEC §3.1.
//!
//! After Stage 1's three oracles (catalog, tests, NatSpec) produce
//! candidate intents and the critique pass filters out vacuous /
//! restate-the-source candidates, the surviving intents flow through
//! this gate before the V1 CEGIS loop runs against them. The user
//! confirms, skips, edits, or all-confirms each proposed intent.
//!
//! Why it exists: the LLM's plain-English proposals are the ONE
//! human-in-the-loop step that distinguishes V1.5 from a fully-
//! autonomous "trust the LLM" pipeline (SPEC §2 / §3.6). It's also
//! the liability shield — a user who confirmed five English invariants
//! about their own contract can't claim Vergil verified something the
//! user didn't agree to.
//!
//! V1.5 uses an on-disk state file (`vergil-out/confirm/state.json`)
//! rather than the V2 `JobStatus::AwaitingConfirmation` enum extension
//! (SPEC §3.1, §11.6 explicitly defers the SaaS-side wrap). Resumability
//! lives in the state file's `decisions` log; `--resume` reads it and
//! skips intents already decided.
//!
//! Three driving modes:
//!
//! - **AutoYes** — `--yes` flag on. No human pause; every proposed
//!   intent confirmed. The agent / CI path.
//! - **TtyInteractive** — TTY attached. Per-intent `[c]onfirm /
//!   [s]kip / [e]dit / [a]ll-yes` prompt. Decisions persist
//!   incrementally so a Ctrl-C in the middle leaves a partially-
//!   decided state file `--resume` can pick up.
//! - **JsonExchange** — non-TTY (stdout is piped). Read JSON
//!   decisions from stdin; write JSON prompts to stdout. SPEC §3.1's
//!   "AI agent can auto-confirm" non-interactive path.
//!
//! Slice 8's orchestrator constructs the gate, hands it the post-
//! critique intent list, and drives it via [`run_gate`]. The result is
//! the subset of intents that will feed the Stage 3 CEGIS path.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::synthesis::Source;

/// Schema version of `state.json`. Bump on incompat changes; the
/// loader rejects unknown versions so Phase 6 can't accidentally
/// resume a future-format state file.
pub const CONFIRM_STATE_SCHEMA_VERSION: u32 = 1;

/// One LLM-proposed intent the user must accept / skip / edit. Carries
/// enough context (rationale, confidence, provenance) for the user
/// to make an informed call. Slice 8's runner builds these from the
/// catalog / tests / NatSpec oracle outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedIntent {
    /// Stable identifier — used as the key for resume decisions. Slice
    /// 8 forms this as `{source_label}:{intent_name}` so it survives
    /// across re-runs of the same project.
    pub id: String,
    pub source: Source,
    /// Single English sentence stating the candidate property.
    pub intent_text: String,
    /// One-line audit-trail rationale — how the LLM derived this
    /// intent from its source (test body, doc comment, catalog
    /// negation_property).
    pub rationale: String,
    /// Per-intent critique confidence (the post-critique 4-axis min).
    pub confidence: f32,
    /// Catalog template id when `source == AttackCatalog`. None
    /// otherwise.
    pub template_ref: Option<String>,
}

/// User decision for one [`ProposedIntent`]. `Edit` carries the
/// replacement text; the runner uses it instead of `intent_text`
/// downstream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Decision {
    Confirm,
    Skip,
    Edit { new_text: String },
}

/// Persisted per-decision record. The decisions vector in
/// [`ConfirmState`] is append-only so a `--resume` run reads the
/// already-decided IDs and skips re-presenting them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub intent_id: String,
    pub decision: Decision,
    pub decided_at: DateTime<Utc>,
}

/// Gate lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmStatus {
    /// No intent has been decided yet.
    Pending,
    /// At least one intent is still undecided. A killed run will
    /// leave the state file in this status for `--resume` to pick up.
    InProgress,
    /// Every intent has been decided.
    Complete,
}

/// On-disk persistence. Lives at
/// `<project>/vergil-out/confirm/state.json` (Slice 4's
/// `layout::confirm_state`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfirmState {
    pub schema_version: u32,
    pub run_id: String,
    pub proposed_intents: Vec<ProposedIntent>,
    pub decisions: Vec<DecisionRecord>,
    pub status: ConfirmStatus,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ConfirmState {
    pub fn new(run_id: String, intents: Vec<ProposedIntent>, now: DateTime<Utc>) -> Self {
        Self {
            schema_version: CONFIRM_STATE_SCHEMA_VERSION,
            run_id,
            proposed_intents: intents,
            decisions: Vec::new(),
            status: ConfirmStatus::Pending,
            started_at: now,
            updated_at: now,
        }
    }

    pub fn decided_ids(&self) -> std::collections::BTreeSet<String> {
        self.decisions
            .iter()
            .map(|d| d.intent_id.clone())
            .collect()
    }

    pub fn undecided(&self) -> impl Iterator<Item = &ProposedIntent> {
        let decided = self.decided_ids();
        self.proposed_intents
            .iter()
            .filter(move |i| !decided.contains(&i.id))
    }

    /// Final set of (intent, decision) pairs. Slice 8's runner reads
    /// this after gate close to determine which intents feed Stage 3.
    pub fn confirmed_intents(&self) -> Vec<(ProposedIntent, Decision)> {
        let mut out = Vec::new();
        for d in &self.decisions {
            let Some(intent) = self.proposed_intents.iter().find(|i| i.id == d.intent_id)
            else {
                continue;
            };
            match &d.decision {
                Decision::Confirm | Decision::Edit { .. } => {
                    out.push((intent.clone(), d.decision.clone()));
                }
                Decision::Skip => {}
            }
        }
        out
    }
}

#[derive(Debug, Error)]
pub enum ConfirmError {
    #[error("read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("schema_version {got} unsupported (expected {expected})")]
    SchemaMismatch { got: u32, expected: u32 },
    #[error("run_id mismatch on resume: state file has {state}, current run is {current}")]
    RunIdMismatch { state: String, current: String },
    #[error("prompt I/O error: {0}")]
    Prompt(#[source] std::io::Error),
    #[error("invalid agent JSON decision: {0}")]
    InvalidAgentDecision(String),
}

/// Persist the state file atomically (write to .tmp, fsync, rename).
pub fn save_state(path: &Path, state: &ConfirmState) -> Result<(), ConfirmError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ConfirmError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_string_pretty(state).map_err(|e| ConfirmError::Parse {
        path: path.to_path_buf(),
        source: e,
    })?;
    std::fs::write(&tmp, body).map_err(|e| ConfirmError::Io {
        path: tmp.clone(),
        source: e,
    })?;
    std::fs::rename(&tmp, path).map_err(|e| ConfirmError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

/// Read the state file, rejecting unknown schema versions.
pub fn load_state(path: &Path) -> Result<ConfirmState, ConfirmError> {
    let body = std::fs::read_to_string(path).map_err(|e| ConfirmError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let state: ConfirmState = serde_json::from_str(&body).map_err(|e| ConfirmError::Parse {
        path: path.to_path_buf(),
        source: e,
    })?;
    if state.schema_version != CONFIRM_STATE_SCHEMA_VERSION {
        return Err(ConfirmError::SchemaMismatch {
            got: state.schema_version,
            expected: CONFIRM_STATE_SCHEMA_VERSION,
        });
    }
    Ok(state)
}

/// Gate driving mode chosen by Slice 8's orchestrator from CLI flags.
pub enum GateMode<'a> {
    /// `--yes`: auto-confirm every intent. No prompts, no I/O.
    AutoYes,
    /// Interactive TTY: per-intent prompt via the supplied prompter.
    Tty {
        prompter: &'a mut dyn TtyPrompter,
    },
    /// Non-TTY: JSON I/O for agent callers. Reads JSON decisions
    /// from `reader`, writes JSON prompts to `writer`.
    Json {
        reader: &'a mut dyn BufRead,
        writer: &'a mut dyn Write,
    },
}

/// Trait for the TTY interactive prompter. Injecting a trait keeps
/// the unit tests deterministic — the production driver reads from
/// stdin / writes to stderr; tests supply scripted decisions.
pub trait TtyPrompter {
    fn prompt(&mut self, intent: &ProposedIntent) -> Result<Decision, ConfirmError>;
    /// Set after the user selects [a]ll-yes mid-list. Subsequent
    /// calls short-circuit to `Decision::Confirm`.
    fn all_yes_armed(&self) -> bool;
}

/// Drive the gate from `start_state` to completion using `mode`. The
/// state file at `state_path` is updated incrementally — a Ctrl-C
/// between decisions leaves a partial state that `--resume` reads.
///
/// Returns the final state. Slice 8 reads `final.confirmed_intents()`
/// to drive Stage 3.
pub fn run_gate(
    state_path: &Path,
    start_state: ConfirmState,
    mode: GateMode<'_>,
    now: DateTime<Utc>,
) -> Result<ConfirmState, ConfirmError> {
    let mut state = start_state;
    if state.proposed_intents.is_empty() {
        state.status = ConfirmStatus::Complete;
        state.updated_at = now;
        save_state(state_path, &state)?;
        return Ok(state);
    }
    state.status = ConfirmStatus::InProgress;
    save_state(state_path, &state)?;

    match mode {
        GateMode::AutoYes => {
            let pending: Vec<ProposedIntent> = state.undecided().cloned().collect();
            for intent in pending {
                state.decisions.push(DecisionRecord {
                    intent_id: intent.id.clone(),
                    decision: Decision::Confirm,
                    decided_at: now,
                });
                state.updated_at = now;
                save_state(state_path, &state)?;
            }
        }
        GateMode::Tty { prompter } => {
            let pending: Vec<ProposedIntent> = state.undecided().cloned().collect();
            for intent in pending {
                let decision = if prompter.all_yes_armed() {
                    Decision::Confirm
                } else {
                    prompter.prompt(&intent)?
                };
                state.decisions.push(DecisionRecord {
                    intent_id: intent.id.clone(),
                    decision,
                    decided_at: now,
                });
                state.updated_at = now;
                save_state(state_path, &state)?;
            }
        }
        GateMode::Json { reader, writer } => {
            let pending: Vec<ProposedIntent> = state.undecided().cloned().collect();
            for intent in pending {
                let prompt = serde_json::json!({
                    "kind": "intent_prompt",
                    "id": intent.id,
                    "source": source_wire(intent.source),
                    "intent_text": intent.intent_text,
                    "rationale": intent.rationale,
                    "confidence": intent.confidence,
                    "template_ref": intent.template_ref,
                });
                writeln!(writer, "{prompt}").map_err(ConfirmError::Prompt)?;
                writer.flush().map_err(ConfirmError::Prompt)?;

                let mut line = String::new();
                let n = reader.read_line(&mut line).map_err(ConfirmError::Prompt)?;
                if n == 0 {
                    return Err(ConfirmError::InvalidAgentDecision(
                        "EOF before decision".into(),
                    ));
                }
                let decision: Decision = serde_json::from_str(line.trim()).map_err(|e| {
                    ConfirmError::InvalidAgentDecision(format!("{e}: input was {line:?}"))
                })?;
                state.decisions.push(DecisionRecord {
                    intent_id: intent.id.clone(),
                    decision,
                    decided_at: now,
                });
                state.updated_at = now;
                save_state(state_path, &state)?;
            }
        }
    }

    state.status = ConfirmStatus::Complete;
    state.updated_at = now;
    save_state(state_path, &state)?;
    Ok(state)
}

fn source_wire(s: Source) -> &'static str {
    match s {
        Source::UserIntent => "user_intent",
        Source::AttackCatalog => "attack_catalog",
        Source::Conformance => "conformance",
        Source::Tests => "tests",
        Source::NatSpec => "nat_spec",
        Source::Structural => "structural",
    }
}

/// Resume helper: if a state file exists, load it and merge with the
/// fresh intent list (preserving existing decisions for re-presented
/// intent IDs). When the run_id mismatches, treat the resume as
/// starting from scratch (different project / different run).
pub fn resume_or_new(
    state_path: &Path,
    run_id: &str,
    fresh_intents: Vec<ProposedIntent>,
    now: DateTime<Utc>,
) -> Result<ConfirmState, ConfirmError> {
    if !state_path.is_file() {
        return Ok(ConfirmState::new(run_id.to_string(), fresh_intents, now));
    }
    let existing = load_state(state_path)?;
    if existing.run_id != run_id {
        return Err(ConfirmError::RunIdMismatch {
            state: existing.run_id,
            current: run_id.to_string(),
        });
    }
    // Carry over decisions for intent IDs still present in the fresh
    // list. Decisions for IDs no longer proposed (rare — intent set
    // shifted between runs) are dropped.
    let fresh_ids: std::collections::BTreeSet<String> =
        fresh_intents.iter().map(|i| i.id.clone()).collect();
    let surviving_decisions: Vec<DecisionRecord> = existing
        .decisions
        .into_iter()
        .filter(|d| fresh_ids.contains(&d.intent_id))
        .collect();
    let mut merged = ConfirmState::new(run_id.to_string(), fresh_intents, existing.started_at);
    merged.decisions = surviving_decisions;
    merged.updated_at = now;
    merged.status = if merged.decisions.len() == merged.proposed_intents.len() {
        ConfirmStatus::Complete
    } else if merged.decisions.is_empty() {
        ConfirmStatus::Pending
    } else {
        ConfirmStatus::InProgress
    };
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    fn now() -> DateTime<Utc> {
        // Fixed timestamp for deterministic snapshots — never call
        // chrono::Utc::now() in tests.
        DateTime::parse_from_rfc3339("2026-06-02T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn sample_intent(id: &str, src: Source) -> ProposedIntent {
        ProposedIntent {
            id: id.to_string(),
            source: src,
            intent_text: format!("English invariant for {id}"),
            rationale: format!("derived from {id} source"),
            confidence: 0.75,
            template_ref: match src {
                Source::AttackCatalog => Some("template-abc".to_string()),
                _ => None,
            },
        }
    }

    // ─── Plan §4 Slice 7 acceptance 2: --yes auto-confirms ────────────

    #[test]
    fn auto_yes_confirms_every_intent_without_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        let intents = vec![
            sample_intent("a", Source::AttackCatalog),
            sample_intent("b", Source::Tests),
            sample_intent("c", Source::NatSpec),
        ];
        let state = ConfirmState::new("run-1".into(), intents, now());
        let final_state =
            run_gate(&path, state, GateMode::AutoYes, now()).expect("auto-yes gate");
        assert_eq!(final_state.status, ConfirmStatus::Complete);
        assert_eq!(final_state.decisions.len(), 3);
        for d in &final_state.decisions {
            assert_eq!(d.decision, Decision::Confirm);
        }
        assert_eq!(final_state.confirmed_intents().len(), 3);
    }

    // ─── Plan §4 Slice 7 acceptance 3: --resume picks up mid-flight ──

    #[test]
    fn resume_skips_already_decided_intents() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        let intents = vec![
            sample_intent("a", Source::Tests),
            sample_intent("b", Source::NatSpec),
            sample_intent("c", Source::AttackCatalog),
        ];
        // Simulate a killed mid-run: decide `a`, persist, then die.
        let mut state = ConfirmState::new("run-x".into(), intents.clone(), now());
        state.status = ConfirmStatus::InProgress;
        state.decisions.push(DecisionRecord {
            intent_id: "a".to_string(),
            decision: Decision::Confirm,
            decided_at: now(),
        });
        save_state(&path, &state).unwrap();

        // Resume: load + run with auto-yes to drive the remaining.
        let resumed = resume_or_new(&path, "run-x", intents, now()).unwrap();
        assert_eq!(resumed.decisions.len(), 1);
        assert_eq!(resumed.status, ConfirmStatus::InProgress);
        let final_state =
            run_gate(&path, resumed, GateMode::AutoYes, now()).expect("resume gate");
        assert_eq!(final_state.decisions.len(), 3);
        // The pre-existing decision for `a` must still be Confirm.
        let a = final_state
            .decisions
            .iter()
            .find(|d| d.intent_id == "a")
            .unwrap();
        assert_eq!(a.decision, Decision::Confirm);
    }

    #[test]
    fn resume_rejects_state_file_with_different_run_id() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        let state = ConfirmState::new(
            "previous-run".into(),
            vec![sample_intent("a", Source::Tests)],
            now(),
        );
        save_state(&path, &state).unwrap();
        let err = resume_or_new(
            &path,
            "different-run",
            vec![sample_intent("a", Source::Tests)],
            now(),
        )
        .unwrap_err();
        assert!(matches!(err, ConfirmError::RunIdMismatch { .. }));
    }

    #[test]
    fn resume_without_existing_state_returns_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        let intents = vec![sample_intent("a", Source::Tests)];
        let state = resume_or_new(&path, "fresh", intents, now()).unwrap();
        assert_eq!(state.status, ConfirmStatus::Pending);
        assert!(state.decisions.is_empty());
    }

    // ─── Plan §4 Slice 7 acceptance 4: non-TTY JSON I/O ──────────────

    #[test]
    fn json_io_path_reads_decisions_from_reader_and_writes_prompts_to_writer() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        let intents = vec![
            sample_intent("a", Source::AttackCatalog),
            sample_intent("b", Source::Tests),
        ];
        let state = ConfirmState::new("run-1".into(), intents, now());
        // Agent script: confirm a, edit b.
        let scripted = "{\"kind\":\"confirm\"}\n{\"kind\":\"edit\",\"new_text\":\"refined\"}\n";
        let mut reader = Cursor::new(scripted.as_bytes());
        let mut writer = Vec::<u8>::new();
        let final_state = run_gate(
            &path,
            state,
            GateMode::Json {
                reader: &mut reader,
                writer: &mut writer,
            },
            now(),
        )
        .expect("json gate");
        assert_eq!(final_state.decisions.len(), 2);
        assert_eq!(final_state.decisions[0].decision, Decision::Confirm);
        assert_eq!(
            final_state.decisions[1].decision,
            Decision::Edit {
                new_text: "refined".into()
            }
        );
        // Two prompts written to stdout (one per intent).
        let out = String::from_utf8(writer).unwrap();
        assert_eq!(out.matches("intent_prompt").count(), 2);
        assert!(out.contains("\"id\":\"a\""));
        assert!(out.contains("\"id\":\"b\""));
    }

    #[test]
    fn json_io_rejects_malformed_decision_input() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        let intents = vec![sample_intent("a", Source::Tests)];
        let state = ConfirmState::new("run-1".into(), intents, now());
        let mut reader = Cursor::new(b"not json\n".as_slice());
        let mut writer = Vec::<u8>::new();
        let err = run_gate(
            &path,
            state,
            GateMode::Json {
                reader: &mut reader,
                writer: &mut writer,
            },
            now(),
        )
        .unwrap_err();
        assert!(matches!(err, ConfirmError::InvalidAgentDecision(_)));
    }

    // ─── Plan §4 Slice 7 acceptance 1: TTY interactive prompt ────────

    struct ScriptedPrompter {
        decisions: Vec<Decision>,
        all_yes_after: usize,
        cursor: usize,
    }

    impl TtyPrompter for ScriptedPrompter {
        fn prompt(&mut self, _intent: &ProposedIntent) -> Result<Decision, ConfirmError> {
            let d = self.decisions[self.cursor].clone();
            self.cursor += 1;
            Ok(d)
        }
        fn all_yes_armed(&self) -> bool {
            self.cursor >= self.all_yes_after
        }
    }

    #[test]
    fn tty_interactive_collects_each_decision_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        let intents = vec![
            sample_intent("a", Source::Tests),
            sample_intent("b", Source::NatSpec),
            sample_intent("c", Source::AttackCatalog),
        ];
        let mut prompter = ScriptedPrompter {
            decisions: vec![
                Decision::Confirm,
                Decision::Skip,
                Decision::Edit {
                    new_text: "fix".into(),
                },
            ],
            all_yes_after: usize::MAX,
            cursor: 0,
        };
        let state = ConfirmState::new("run-1".into(), intents, now());
        let final_state = run_gate(
            &path,
            state,
            GateMode::Tty {
                prompter: &mut prompter,
            },
            now(),
        )
        .unwrap();
        assert_eq!(final_state.decisions[0].decision, Decision::Confirm);
        assert_eq!(final_state.decisions[1].decision, Decision::Skip);
        assert!(matches!(
            final_state.decisions[2].decision,
            Decision::Edit { .. }
        ));
        // Skipped intent isn't in confirmed_intents().
        assert_eq!(final_state.confirmed_intents().len(), 2);
    }

    #[test]
    fn tty_all_yes_armed_short_circuits_remaining_intents() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        let intents = vec![
            sample_intent("a", Source::Tests),
            sample_intent("b", Source::NatSpec),
            sample_intent("c", Source::AttackCatalog),
        ];
        let mut prompter = ScriptedPrompter {
            // Only one explicit decision needed; all_yes triggers
            // after the first.
            decisions: vec![Decision::Confirm],
            all_yes_after: 1,
            cursor: 0,
        };
        let state = ConfirmState::new("run-1".into(), intents, now());
        let final_state = run_gate(
            &path,
            state,
            GateMode::Tty {
                prompter: &mut prompter,
            },
            now(),
        )
        .unwrap();
        assert_eq!(final_state.decisions.len(), 3);
        for d in &final_state.decisions {
            assert_eq!(d.decision, Decision::Confirm);
        }
    }

    // ─── State file shape + persistence ─────────────────────────────

    #[test]
    fn state_file_round_trips_through_save_load() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested/dir/state.json");
        let state = ConfirmState::new(
            "run-1".into(),
            vec![sample_intent("a", Source::AttackCatalog)],
            now(),
        );
        save_state(&path, &state).unwrap();
        let back = load_state(&path).unwrap();
        assert_eq!(back, state);
    }

    #[test]
    fn load_state_rejects_unsupported_schema_version() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        let mut state = ConfirmState::new(
            "run-1".into(),
            vec![sample_intent("a", Source::Tests)],
            now(),
        );
        state.schema_version = 99;
        std::fs::write(&path, serde_json::to_string(&state).unwrap()).unwrap();
        let err = load_state(&path).unwrap_err();
        assert!(matches!(err, ConfirmError::SchemaMismatch { .. }));
    }

    #[test]
    fn empty_intent_list_completes_immediately() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        let state = ConfirmState::new("run-1".into(), Vec::new(), now());
        let final_state =
            run_gate(&path, state, GateMode::AutoYes, now()).expect("empty gate");
        assert_eq!(final_state.status, ConfirmStatus::Complete);
        assert!(final_state.decisions.is_empty());
        assert!(final_state.confirmed_intents().is_empty());
    }

    #[test]
    fn save_state_is_atomic_via_tmp_rename() {
        // Verify the .tmp file is not left behind after a successful
        // save_state (atomicity invariant: rename is the visibility
        // boundary).
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        let state = ConfirmState::new(
            "run-1".into(),
            vec![sample_intent("a", Source::Tests)],
            now(),
        );
        save_state(&path, &state).unwrap();
        let tmp_file = path.with_extension("json.tmp");
        assert!(!tmp_file.exists(), ".tmp file must not survive a successful save");
        assert!(path.is_file());
    }
}
