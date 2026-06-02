//! Structured telemetry — Phase 4 Slice B2.
//!
//! Replaces ad-hoc `eprintln!` and untyped `tracing::warn!` with
//! structured events V2's billing + observability layers can consume
//! directly. Two output channels:
//!
//! 1. `tracing` spans + events with structured fields (always emitted
//!    when `tracing_subscriber` is set up; no extra work in the CLI).
//! 2. Optional JSONL stream to a file via [`JsonlSink`] — the CLI
//!    enables this with `--telemetry-json <path>`. V2's billing layer
//!    reads the resulting file directly.
//!
//! Tenancy: every event carries `tenant_id` (defaulting to `"internal"`
//! in the CLI). V2 wires real per-customer tenant IDs from the
//! `AuthIdentity` returned by `vergil-service`'s auth layer.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// One structured telemetry event. Shape is stable across slice changes
/// because V2's billing layer pins on the field names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    /// ISO-8601 UTC timestamp of when the event fired.
    pub timestamp: String,
    /// Stable per-tenant identifier. CLI default: `"internal"`.
    pub tenant_id: String,
    /// Run identifier so V2 can group events by CEGIS run.
    pub run_id: String,
    /// CEGIS iteration index (0-based); 0 for non-iteration events.
    pub iteration: usize,
    /// Event kind — one of `synth_sample`, `critique_summary`,
    /// `mutation_summary`, `dispatch_summary`, `cost`, `wall_clock`,
    /// `run_complete`.
    pub kind: String,
    /// Free-form structured payload. Documented per kind in the
    /// `EventKind` constants below.
    pub fields: serde_json::Value,
}

/// Stable string identifiers for event kinds. V2's billing layer
/// matches on these literal strings; do not rename without bumping the
/// telemetry schema version (which V2 reads from the first line).
pub mod kind {
    pub const SYNTH_SAMPLE: &str = "synth_sample";
    pub const CRITIQUE_SUMMARY: &str = "critique_summary";
    pub const MUTATION_SUMMARY: &str = "mutation_summary";
    pub const DISPATCH_SUMMARY: &str = "dispatch_summary";
    pub const COST: &str = "cost";
    pub const WALL_CLOCK: &str = "wall_clock";
    pub const RUN_COMPLETE: &str = "run_complete";
    /// V1.5 Phase 6 Slice 6 — emitted by the cex sink the moment a
    /// counterexample file is written to disk. Fields: `property`
    /// (string), `source` (one of the Source variants), `template_ref`
    /// (optional string), `cex_file` (relative path inside vergil-out/).
    pub const COUNTEREXAMPLE_FOUND: &str = "counterexample_found";
    /// V1.5 Phase 5 — emitted once per `extract_from_structural` run.
    /// Fields: `total_candidates` (high-confidence, ≥ min_confidence,
    /// flow into the pipeline), `low_confidence_findings` (report-only),
    /// `by_miner` (object mapping miner id → count of emitted candidates).
    pub const STRUCTURAL_CANDIDATES_EMITTED: &str = "structural_candidates_emitted";
}

/// Telemetry sink trait. Implementations: [`JsonlSink`] (file-backed),
/// [`NullSink`] (drops events; used when `--telemetry-json` isn't set),
/// and test fakes.
pub trait TelemetrySink: Send + Sync {
    fn record(&self, event: &TelemetryEvent);
}

/// Drops every event. Default sink when telemetry JSONL output isn't
/// requested by the CLI.
#[derive(Debug, Default)]
pub struct NullSink;

impl TelemetrySink for NullSink {
    fn record(&self, _event: &TelemetryEvent) {
        // Intentional no-op.
    }
}

/// Appends one JSON line per event to a file. Thread-safe via Mutex.
pub struct JsonlSink {
    path: PathBuf,
    file: Mutex<std::fs::File>,
}

impl JsonlSink {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            path,
            file: Mutex::new(file),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl TelemetrySink for JsonlSink {
    fn record(&self, event: &TelemetryEvent) {
        let Ok(serialized) = serde_json::to_string(event) else {
            // Serialization should never fail on our own types, but if
            // it does, log via tracing and drop the event.
            tracing::error!("telemetry: failed to serialize event {event:?}");
            return;
        };
        let Ok(mut file) = self.file.lock() else {
            tracing::error!("telemetry: sink mutex poisoned");
            return;
        };
        if let Err(e) = writeln!(file, "{serialized}") {
            tracing::error!("telemetry: write to {} failed: {e}", self.path.display());
        }
    }
}

/// Per-run cost accounting. V2's billing layer aggregates this across
/// jobs to produce per-tenant invoices. Phase 4 ships the shape;
/// CegisLoop populates it; the CLI emits one `cost` event per run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostAccounting {
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub usd_estimate: f64,
    pub wall_clock_ms: u64,
}

impl CostAccounting {
    /// Render as a JSON `Value` for inclusion in [`TelemetryEvent::fields`].
    pub fn as_fields(&self) -> serde_json::Value {
        serde_json::json!({
            "tokens_in": self.tokens_in,
            "tokens_out": self.tokens_out,
            "usd_estimate": self.usd_estimate,
            "wall_clock_ms": self.wall_clock_ms,
        })
    }
}

/// Build a [`TelemetryEvent`] with timestamp + tenant_id + run_id set.
pub fn event(
    tenant_id: &str,
    run_id: &str,
    iteration: usize,
    kind: &str,
    fields: serde_json::Value,
) -> TelemetryEvent {
    TelemetryEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        tenant_id: tenant_id.to_string(),
        run_id: run_id.to_string(),
        iteration,
        kind: kind.to_string(),
        fields,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_round_trips_through_json() {
        let e = event(
            "acme",
            "run-123",
            2,
            kind::CRITIQUE_SUMMARY,
            serde_json::json!({"accepted": 5, "total": 8}),
        );
        let json = serde_json::to_string(&e).unwrap();
        let back: TelemetryEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tenant_id, "acme");
        assert_eq!(back.run_id, "run-123");
        assert_eq!(back.iteration, 2);
        assert_eq!(back.kind, "critique_summary");
        assert_eq!(back.fields["accepted"], 5);
    }

    #[test]
    fn jsonl_sink_appends_one_line_per_event() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("events.jsonl");
        let sink = JsonlSink::open(&path).unwrap();
        for i in 0..3 {
            sink.record(&event(
                "internal",
                "r",
                i,
                kind::SYNTH_SAMPLE,
                serde_json::json!({"i": i}),
            ));
        }
        let body = std::fs::read_to_string(&path).unwrap();
        assert_eq!(body.lines().count(), 3);
        // Each line should round-trip as a TelemetryEvent.
        for line in body.lines() {
            let _e: TelemetryEvent = serde_json::from_str(line).unwrap();
        }
    }

    #[test]
    fn null_sink_is_a_no_op() {
        let s = NullSink;
        s.record(&event("t", "r", 0, "kind", serde_json::Value::Null));
        // No panic, no file written. Implicit test: this compiles + runs.
    }

    #[test]
    fn jsonl_sink_creates_parent_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("deep/nested/events.jsonl");
        let sink = JsonlSink::open(&nested).unwrap();
        sink.record(&event("t", "r", 0, "k", serde_json::json!({})));
        assert!(nested.exists());
    }

    #[test]
    fn cost_accounting_renders_as_fields() {
        let c = CostAccounting {
            tokens_in: 100,
            tokens_out: 50,
            usd_estimate: 0.42,
            wall_clock_ms: 1234,
        };
        let v = c.as_fields();
        assert_eq!(v["tokens_in"], 100);
        assert_eq!(v["usd_estimate"], 0.42);
    }
}
