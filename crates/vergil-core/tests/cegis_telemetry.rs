//! Phase 4 Slice B2: CegisLoop telemetry wiring.
//!
//! Stubs the LLM provider + dispatcher so the loop terminates on the
//! first iteration with a verified verdict, and asserts the sink saw
//! every expected event kind. Also exercises the JsonlSink end-to-end:
//! a real CegisLoop run with the file-backed sink must produce parseable
//! JSON lines.

use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;

use vergil_core::cegis::{CegisConfig, CegisLoop, VerifierDispatcher, VerifierVerdict};
use vergil_core::critique::{Critic, CritiqueConfig};
use vergil_core::diagnosis::{DiagnosisConfig, Diagnostician};
use vergil_core::refinement::{RefinementConfig, Refiner};
use vergil_core::synthesis::{SpecCandidate, StaticAnalysisSummary};
use vergil_core::telemetry::{kind, JsonlSink, TelemetryEvent, TelemetrySink};
use vergil_llm::{
    Completion, CompletionRequest, EmbedRequest, Embedding, LlmError, LlmProvider, ProviderId,
    StructuredRequest, StructuredResponse,
};

/// Provider stub. Synth returns one valid candidate; critique accepts
/// with high scores; embeddings irrelevant (the test bypasses retrieval).
struct StubProvider;

#[async_trait]
impl LlmProvider for StubProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Mock
    }
    async fn complete(&self, _req: CompletionRequest) -> Result<Completion, LlmError> {
        Ok(Completion {
            content: r#"[{"name":"check_x","halmos":"function check_x() public {}","smtchecker":"","template_ref":null,"intent_satisfied":true}]"#.to_string(),
            tokens_in: 100,
            tokens_out: 50,
            latency_ms: 10,
            provider_request_id: Some("stub-complete".to_string()),
        })
    }
    async fn complete_structured(
        &self,
        _req: StructuredRequest,
    ) -> Result<StructuredResponse, LlmError> {
        Ok(StructuredResponse {
            value: serde_json::json!({
                "verdict": "accept",
                "scores": { "vacuity": 0.9, "body_independence": 0.9, "testability": 0.9 },
                "rationale": "stub accept"
            }),
            tokens_in: 50,
            tokens_out: 25,
            latency_ms: 5,
            provider_request_id: None,
        })
    }
    async fn embed(&self, _req: EmbedRequest) -> Result<Embedding, LlmError> {
        Ok(Embedding {
            vectors: vec![],
            tokens: 0,
            latency_ms: 0,
        })
    }
}

/// Dispatcher that always verifies — the loop exits after one iteration
/// with stop_reason = "verified".
struct AlwaysVerifiedDispatcher;

#[async_trait]
impl VerifierDispatcher for AlwaysVerifiedDispatcher {
    async fn dispatch(&self, _spec: &SpecCandidate) -> VerifierVerdict {
        VerifierVerdict::verified()
    }
}

/// In-memory recording sink so the test can assert on the event stream
/// without touching the filesystem.
#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<TelemetryEvent>>,
}

impl TelemetrySink for RecordingSink {
    fn record(&self, event: &TelemetryEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

fn build_loop(stub: Arc<StubProvider>, sink: Arc<dyn TelemetrySink>) -> CegisLoop {
    CegisLoop {
        synthesizer: stub.clone(),
        critic: Critic::new(stub.clone(), ProviderId::Mock, CritiqueConfig::for_tests()),
        diagnostician: Diagnostician::new(stub.clone(), DiagnosisConfig::for_tests()),
        refiner: Refiner::new(stub.clone(), RefinementConfig::for_tests()),
        mutation_gate: None,
        dispatcher: Arc::new(AlwaysVerifiedDispatcher),
        cfg: CegisConfig::for_tests(),
        mutation_min: 0.4,
        telemetry: sink,
    }
}

#[tokio::test]
async fn cegis_loop_emits_every_expected_event_kind() {
    let stub = Arc::new(StubProvider);
    let recorder = Arc::new(RecordingSink::default());
    let cegis = build_loop(stub, recorder.clone());

    let sa = StaticAnalysisSummary::default();
    let _ = cegis
        .run_with_description("test intent", Some("test desc"), "", &sa, &[], "// src")
        .await
        .expect("run ok");

    let events = recorder.events.lock().unwrap();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    for expected in [
        kind::SYNTH_SAMPLE,
        kind::CRITIQUE_SUMMARY,
        kind::MUTATION_SUMMARY,
        kind::DISPATCH_SUMMARY,
        kind::COST,
        kind::RUN_COMPLETE,
    ] {
        assert!(
            kinds.contains(&expected),
            "telemetry missing expected event kind {expected}: {kinds:?}"
        );
    }
    // Every event carries the test tenant ID from `CegisConfig::for_tests`.
    assert!(events.iter().all(|e| e.tenant_id == "test"));
    // Last event is run_complete and reports one verified property.
    let last = events.last().unwrap();
    assert_eq!(last.kind, kind::RUN_COMPLETE);
    assert_eq!(last.fields["verified"], 1);
}

#[tokio::test]
async fn jsonl_sink_writes_parseable_lines_from_a_cegis_run() {
    let stub = Arc::new(StubProvider);
    let tmp = tempfile::tempdir().unwrap();
    let jsonl_path = tmp.path().join("events.jsonl");
    let sink: Arc<dyn TelemetrySink> = Arc::new(JsonlSink::open(&jsonl_path).unwrap());
    let cegis = build_loop(stub, sink);

    let sa = StaticAnalysisSummary::default();
    let _ = cegis
        .run_with_description("i", None, "", &sa, &[], "src")
        .await
        .expect("run ok");

    let body = std::fs::read_to_string(&jsonl_path).unwrap();
    let lines: Vec<&str> = body.lines().collect();
    assert!(
        lines.len() >= 6,
        "expected at least 6 events on disk; got {} ({body})",
        lines.len()
    );
    for line in &lines {
        let _: TelemetryEvent = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("event line not parseable: {e}; line={line}"));
    }
}
