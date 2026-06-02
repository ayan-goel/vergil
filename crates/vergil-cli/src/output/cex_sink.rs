//! Counterexample streaming sink — V1.5 Phase 6 Slice 6.
//!
//! Per SPEC §3.1, a counterexample must surface "the instant it is
//! found, not after the full sweep." The Phase-1 / V1 path collected
//! every verdict then emitted cex files in a batch; Slice 6 inverts
//! that: each Refuted verdict is written to disk + announced via
//! telemetry + flagged on stderr as soon as it lands.
//!
//! The sink is intentionally narrow:
//!
//!   pub fn emit(record: CounterexampleRecord) -> io::Result<PathBuf>
//!
//! It does NOT generate the cex Solidity source from a Halmos trace —
//! that's still the verify path's responsibility (emit_counterexample
//! in commands/verify.rs). The sink is for routing an already-rendered
//! cex source to its on-disk + telemetry destination. Slice 8's
//! unified runner calls the sink from inside its per-candidate CEGIS
//! loop so each refutation fires immediately.
//!
//! The cex file location stays at `<project>/vergil-out/counterexamples/
//! Cex_<property>.t.sol` per SPEC §3.8 — Slice 4's `layout` helper.
//! Existing erc20-broken regression tests on the cex path keep
//! passing byte-for-byte.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use vergil_core::synthesis::Source;
use vergil_core::telemetry::{event, kind, TelemetrySink};

use crate::output::layout;

/// One refutation, ready to land on disk. The caller (Slice 8's
/// runner) already rendered the Halmos trace into the Solidity test
/// source — the sink is structurally a path-router + telemetry
/// emitter, not a generator.
#[derive(Debug, Clone)]
pub struct CounterexampleRecord {
    /// Property name (Halmos `check_…` suffix). Used as the cex file
    /// stem — `Cex_<name>.t.sol`. Restricted to ASCII alphanumerics
    /// + `_` by the synthesizer; we trust that upstream constraint.
    pub property: String,
    /// Which oracle proposed the property whose negation was refuted.
    pub source: Source,
    /// Catalog template id when `source == AttackCatalog`. `None`
    /// otherwise.
    pub template_ref: Option<String>,
    /// Rendered counterexample Solidity source — what to write to
    /// `Cex_<property>.t.sol`.
    pub source_sol: String,
    /// Optional one-line trace summary the verdict formatter (Slice 5)
    /// will surface. Slice 8 captures from Halmos trace summary.
    pub trace_summary: String,
}

/// Telemetry-friendly view of a source value. Mirrors the
/// `Source` enum's snake_case wire format so the JSONL events are
/// stable for V2's billing pin (SPEC §11 carry-over).
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

/// Streaming counterexample writer. Slice 8's orchestrator constructs
/// one per `vergil verify` run; each refutation calls `emit`.
pub struct CexSink {
    project: PathBuf,
    sink: Arc<dyn TelemetrySink>,
    tenant_id: String,
    run_id: String,
}

impl CexSink {
    pub fn new(
        project: impl Into<PathBuf>,
        sink: Arc<dyn TelemetrySink>,
        tenant_id: impl Into<String>,
        run_id: impl Into<String>,
    ) -> Self {
        Self {
            project: project.into(),
            sink,
            tenant_id: tenant_id.into(),
            run_id: run_id.into(),
        }
    }

    /// Write the counterexample file to disk, emit the
    /// `counterexample_found` telemetry event, and print a stderr
    /// notice for human visibility. The function returns AFTER the
    /// file is written and the telemetry event is recorded, so the
    /// "first cex in 60s" invariant of SPEC §3.1 is observable from
    /// outside (a subsequent verification step starts strictly later).
    ///
    /// Returns the absolute path of the cex file on disk.
    pub fn emit(&self, record: CounterexampleRecord) -> io::Result<PathBuf> {
        let dir = layout::counterexamples_dir(&self.project);
        std::fs::create_dir_all(&dir)?;
        let file = dir.join(format!("Cex_{}.t.sol", record.property));
        std::fs::write(&file, &record.source_sol)?;

        // Per-project file path the verdict formatter (Slice 5) and
        // the `Reproduce` section reference. Relative form keeps the
        // report portable.
        let relative = file
            .strip_prefix(&self.project)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| file.display().to_string());

        let evt = event(
            self.tenant_id.as_str(),
            self.run_id.as_str(),
            0,
            kind::COUNTEREXAMPLE_FOUND,
            serde_json::json!({
                "property": record.property,
                "source": source_wire(record.source),
                "template_ref": record.template_ref,
                "cex_file": relative,
                "trace_summary": record.trace_summary,
            }),
        );
        self.sink.record(&evt);

        // Stderr notice — the user is watching the terminal and the
        // CEX file landing first thing they see is the moat.
        eprintln!(
            "[CEX] {} (source: {}{}) → {}",
            record.property,
            source_wire(record.source),
            record
                .template_ref
                .as_deref()
                .map(|t| format!(", template: {t}"))
                .unwrap_or_default(),
            relative,
        );

        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    use vergil_core::telemetry::TelemetryEvent;

    /// In-memory sink that records every event in order. Tests assert
    /// on the recorded sequence to verify ordering invariants per
    /// plan §4 Slice 6 acceptance criterion 2 (cex_found timestamp
    /// strictly precedes the next dispatch_summary / run_complete).
    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<TelemetryEvent>>,
    }

    impl TelemetrySink for RecordingSink {
        fn record(&self, event: &TelemetryEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    fn make_sink(project: &std::path::Path, recorder: Arc<RecordingSink>) -> CexSink {
        CexSink::new(project, recorder, "test-tenant", "run-1")
    }

    fn sample_record(name: &str) -> CounterexampleRecord {
        CounterexampleRecord {
            property: name.to_string(),
            source: Source::AttackCatalog,
            template_ref: Some("access-public-burn-mint".to_string()),
            source_sol: format!(
                "// SPDX-License-Identifier: UNLICENSED\npragma solidity 0.8.20;\ncontract {name}_Cex {{}}\n"
            ),
            trace_summary: "attacker mints supply".to_string(),
        }
    }

    #[test]
    fn emit_writes_cex_file_at_layout_spec_path() {
        let tmp = tempfile::tempdir().unwrap();
        let recorder = Arc::new(RecordingSink::default());
        let sink = make_sink(tmp.path(), recorder.clone());
        let path = sink.emit(sample_record("check_unauthorized_mint")).unwrap();
        assert_eq!(
            path,
            tmp.path()
                .join("vergil-out/counterexamples/Cex_check_unauthorized_mint.t.sol")
        );
        let body = std::fs::read_to_string(&path).expect("read cex file");
        assert!(body.contains("check_unauthorized_mint_Cex"));
    }

    #[test]
    fn emit_records_counterexample_found_telemetry_event() {
        let tmp = tempfile::tempdir().unwrap();
        let recorder = Arc::new(RecordingSink::default());
        let sink = make_sink(tmp.path(), recorder.clone());
        sink.emit(sample_record("check_x")).unwrap();
        let events = recorder.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        let evt = &events[0];
        assert_eq!(evt.kind, kind::COUNTEREXAMPLE_FOUND);
        assert_eq!(evt.fields["property"], "check_x");
        assert_eq!(evt.fields["source"], "attack_catalog");
        assert_eq!(evt.fields["template_ref"], "access-public-burn-mint");
        assert_eq!(
            evt.fields["cex_file"],
            "vergil-out/counterexamples/Cex_check_x.t.sol"
        );
        assert_eq!(evt.fields["trace_summary"], "attacker mints supply");
    }

    #[test]
    fn emit_writes_file_before_returning() {
        // The streaming invariant: by the time emit() returns, the
        // file MUST exist on disk and the telemetry event MUST be
        // recorded. A subsequent verification step (in Slice 8's
        // unified runner) starts only after this return, so observing
        // a written cex before run_complete is mechanical.
        let tmp = tempfile::tempdir().unwrap();
        let recorder = Arc::new(RecordingSink::default());
        let sink = make_sink(tmp.path(), recorder.clone());
        let path = sink.emit(sample_record("check_y")).unwrap();
        assert!(
            path.exists(),
            "cex file must be on disk before emit returns"
        );
        assert_eq!(
            recorder.events.lock().unwrap().len(),
            1,
            "telemetry event must be recorded before emit returns"
        );
    }

    #[test]
    fn streaming_invariant_first_cex_lands_before_second_emit_starts() {
        // Plan §4 Slice 6 acceptance 3: 2 candidates, one cex'ing
        // first; the first file is on disk before the second emit
        // starts. With a synchronous sink that's tautological — but
        // pin the invariant in a test so Slice 8 doesn't accidentally
        // batch.
        let tmp = tempfile::tempdir().unwrap();
        let recorder = Arc::new(RecordingSink::default());
        let sink = make_sink(tmp.path(), recorder.clone());

        let first = sink.emit(sample_record("check_first")).unwrap();
        // Strict ordering check: before kicking off the second cex,
        // the first must be on disk + telemetry must have it.
        assert!(first.exists());
        let evts = recorder.events.lock().unwrap().clone();
        assert_eq!(evts.len(), 1);
        assert_eq!(evts[0].fields["property"], "check_first");
        drop(evts);

        let second = sink.emit(sample_record("check_second")).unwrap();
        assert!(second.exists());
        let final_events = recorder.events.lock().unwrap();
        assert_eq!(final_events.len(), 2);
        assert_eq!(final_events[0].fields["property"], "check_first");
        assert_eq!(final_events[1].fields["property"], "check_second");
    }

    #[test]
    fn emit_idempotent_on_repeated_writes_same_property() {
        // A re-run of the same cex (e.g. Slice 8 calls emit twice for
        // the same property in a deduped catalog flow) must not error
        // and the file body must reflect the last write.
        let tmp = tempfile::tempdir().unwrap();
        let recorder = Arc::new(RecordingSink::default());
        let sink = make_sink(tmp.path(), recorder.clone());
        let mut rec = sample_record("check_dup");
        rec.source_sol = "// first\n".to_string();
        sink.emit(rec.clone()).unwrap();
        let body1 = std::fs::read_to_string(
            tmp.path()
                .join("vergil-out/counterexamples/Cex_check_dup.t.sol"),
        )
        .unwrap();
        assert!(body1.contains("first"));
        rec.source_sol = "// second\n".to_string();
        sink.emit(rec).unwrap();
        let body2 = std::fs::read_to_string(
            tmp.path()
                .join("vergil-out/counterexamples/Cex_check_dup.t.sol"),
        )
        .unwrap();
        assert!(body2.contains("second"));
    }

    #[test]
    fn emit_handles_non_catalog_sources_with_no_template_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let recorder = Arc::new(RecordingSink::default());
        let sink = make_sink(tmp.path(), recorder.clone());
        let mut rec = sample_record("check_test_derived");
        rec.source = Source::Tests;
        rec.template_ref = None;
        sink.emit(rec).unwrap();
        let events = recorder.events.lock().unwrap();
        assert_eq!(events[0].fields["source"], "tests");
        assert!(events[0].fields["template_ref"].is_null());
    }
}
