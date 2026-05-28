# ADR 0005: Telemetry shape — JSONL events, structured spans, Prometheus stub

**Status:** Accepted (2026-05-27) — Phase 4 Slice B2.
**Decider:** Phase 4 strategic pivot.

## Context

V1 had ad-hoc `eprintln!` + `tracing::warn!` calls. V2's billing layer
needs a stable, parseable event stream for per-tenant cost accounting;
its observability layer needs structured spans for tracing; its ops
layer needs Prometheus-shaped counters for alerting.

## Decision

Three channels, all carrying the same `tenant_id`:

1. **JSONL telemetry stream** via `--telemetry-json <path>` on
   `vergil verify`. One JSON object per line. V2's billing layer
   parses this directly. Schema is owned by `vergil_core::telemetry`.
2. **Tracing spans** with structured fields (`#[instrument]` on the
   CEGIS loop entrypoint, structured fields on every `tracing::info!`
   in the request path). V2 exports these to its tracing backend
   (Honeycomb / Datadog / Tempo / etc.) via the `tracing_opentelemetry`
   adapter.
3. **Prometheus stub** at `GET /metrics` in `vergil-service`. Phase 4
   ships the wire format (text format 0.0.4) + the stable counter
   names; V2 wires real increments.

## Event schema (channel 1: JSONL)

Every event:

```json
{
  "timestamp": "2026-05-27T12:34:56Z",
  "tenant_id": "internal",
  "run_id": "run-20260527T123456Z",
  "iteration": 0,
  "kind": "synth_sample",
  "fields": { /* per-kind payload */ }
}
```

Stable `kind` strings (defined in `vergil_core::telemetry::kind`):

- `synth_sample` — one per LLM sample. Fields: `sample_index`,
  `temperature`, `tokens_in`, `tokens_out`, `latency_ms`,
  `candidate_count`.
- `critique_summary` — one per CEGIS iteration. Fields: `kept`,
  `dropped`.
- `mutation_summary` — one per iteration. Fields: `kept`, `dropped`,
  `mutation_min`.
- `dispatch_summary` — one per iteration. Fields: `dispatched`,
  `verified`, `counterexamples`.
- `cost` — one per run. Fields: `tokens_in`, `tokens_out`,
  `usd_estimate`, `wall_clock_ms`.
- `run_complete` — terminal event. Fields: `iterations`, `outcomes`,
  `verified`, `stop_reason`, `cost_usd`, `wall_clock_ms`.

## Prometheus counter names (channel 3)

Stable (defined in `vergil_service::metrics::counter`):

- `vergil_jobs_total{status=...}` — by terminal status.
- `vergil_telemetry_events_total` — total events ingested.
- `vergil_cost_micro_usd_total` — cumulative cost. Integer counter
  ($1 = 1_000_000 µUSD; integer math beats float-precision in scrape
  diffs).

## Rationale

- **JSONL + tracing covers both audit and observability.** JSONL is
  the durable record; tracing is the live debugging view. Both come
  off the same call sites — no duplication.
- **`tenant_id` is the universal grouping key.** V2's billing layer
  groups by tenant; V2's observability layer filters by tenant;
  V2's auth layer surfaces tenant via `AuthIdentity`. One column to
  pivot every dashboard around.
- **Stable kind strings.** V2's billing layer matches on literals.
  Schema version bump (when we add or rename a kind) gets its own
  ADR.
- **Prometheus stub is honest about being a stub.** Phase 4's
  `/metrics` endpoint emits zero baselines for the stable names so
  V2's scraper can connect on day one and see the series; real
  increments wire up in V2 once jobs actually run through the
  service.

## V2 swap path

- Telemetry sink: V2 swaps the file-backed `JsonlSink` for a streaming
  sink that pushes to its event bus (Kafka, Kinesis, Redis Streams).
  The `TelemetrySink` trait is the seam.
- Tracing exporter: V2 installs `tracing_opentelemetry::layer()` into
  the subscriber and ships spans to its backend.
- Prometheus: V2 wires `state.metrics.inc(...)` calls in the service
  handlers (`submit_job`, status transitions, telemetry ingest).

## Consequences

- Phase 4 ships the shape, V2 wires the bodies. The pattern matches
  ADR 0002 (persistence): trait + stub + real V2 impl.
- Backward compat: the JSONL schema is stable from Phase 4 forward.
  V2 only adds fields (per-event), never removes or renames.
- Storage cost: at ~32 events per CEGIS run × ~200 bytes per event =
  ~6 KB per run. Negligible at any tenant volume V2 cares about.

## References

- `crates/vergil-core/src/telemetry.rs`
- `crates/vergil-service/src/metrics.rs`
- `crates/vergil-core/tests/cegis_telemetry.rs` — integration tests
  asserting every event kind fires + JSONL output is parseable.
