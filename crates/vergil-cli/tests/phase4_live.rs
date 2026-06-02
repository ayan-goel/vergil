//! Phase 4 Slice 7 — SPEC §11.4 exit test against live Anthropic.
//!
//! Gated on `--features llm-live` and the `VERGIL_ANTHROPIC_API_KEY`
//! (or `ANTHROPIC_API_KEY`) environment variable. Both must be present
//! for the test to actually call the LLM; without them the test skips
//! cleanly with an explanatory message (CI default — see SPEC §11
//! "CI is manual-only" / "API-credit exhaustion HALTS the session").
//!
//! ## What this exercises
//!
//! Runs the full Phase 4 extraction pipeline against
//! `examples/vault-4626` with the real Anthropic provider:
//!
//!   1. Parse Foundry / Halmos tests (Slice 1).
//!   2. Parse NatSpec from src/ (Slice 2).
//!   3. Extract test-derived intents via `tests_intent` (Slice 3).
//!   4. Extract NatSpec-derived intents via `natspec_intent` (Slice 4).
//!
//! Asserts the SPEC §11.4 exit-test floor:
//!
//!   * ≥3 candidate intents tagged `source: tests`
//!   * ≥3 candidate intents tagged `source: natspec`
//!   * total LLM cost reported, must stay under the SPEC §11.4 budget
//!     of $1 per contract (the cost is printed for the human runner;
//!     the test does not fail on cost since pricing fluctuates — the
//!     budget gate is the kill-criterion review).
//!
//! ## What this does NOT exercise yet
//!
//! Slice 7 is the extraction-side exit test. Halmos verification of
//! the surviving SpecCandidates lands when the synthesis loop is
//! wired into the CLI's zero-config flow in Phase 6
//! (`vergil verify --mode zero-config`). The plan defers the full
//! synthesize → Halmos → verify chain to Phase 6's standardized
//! workflow.

#![cfg(feature = "llm-live")]

use std::path::Path;
use std::sync::Arc;

use vergil_core::natspec_intent::{
    extract_intent_candidates_from_natspec, NatSpecIntentConfig,
};
use vergil_core::tests_intent::{extract_intent_candidates, TestsIntentConfig};
use vergil_llm::anthropic::AnthropicClient;
use vergil_llm::LlmProvider;
use vergil_solidity::natspec::parse_natspec_dir;
use vergil_solidity::test_parser::parse_tests;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
}

#[tokio::test]
async fn vault_4626_meets_spec_11_4_extraction_floor() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_test_writer()
        .try_init();

    let key = std::env::var("VERGIL_ANTHROPIC_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .ok()
        .filter(|k| !k.is_empty());
    let Some(key) = key else {
        eprintln!(
            "phase4_live: VERGIL_ANTHROPIC_API_KEY / ANTHROPIC_API_KEY not set or empty — \
             skipping. SPEC §11.4 exit test requires a live key per the \
             manual-only-CI policy. If the key lives in a .env file, source \
             it first: `set -a && source .env && set +a` before `cargo test`."
        );
        return;
    };

    let provider: Arc<dyn LlmProvider> =
        Arc::new(AnthropicClient::new(key).expect("anthropic client builds"));

    // Single-call smoke to surface auth / endpoint problems before
    // burning ~9 parallel extraction calls.
    {
        use vergil_llm::{CompletionRequest, Message, Role};
        let smoke_req = CompletionRequest {
            model: "claude-haiku-4-5-20251001".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "Reply with the single word: ok".to_string(),
            }],
            system: Some("You are a terse assistant.".to_string()),
            temperature: 0.0,
            max_tokens: 16,
        };
        match provider.complete(smoke_req).await {
            Ok(resp) => eprintln!("phase4_live: smoke ok ({} tokens out)", resp.tokens_out),
            Err(e) => panic!(
                "phase4_live: smoke call failed before exit test: {e}. \
                 Check VERGIL_ANTHROPIC_API_KEY validity and ANTHROPIC_BASE_URL."
            ),
        }
    }

    let root = workspace_root().join("examples/vault-4626");
    assert!(
        root.exists(),
        "examples/vault-4626 not found at {}",
        root.display()
    );

    let parsed_tests = parse_tests(&root).expect("parse_tests on examples/vault-4626");
    let parsed_natspec = parse_natspec_dir(&root)
        .expect("parse_natspec_dir on examples/vault-4626")
        .into_iter()
        .map(|(_p, b)| b)
        .collect::<Vec<_>>();

    eprintln!(
        "phase4_live: parsed {} tests + {} natspec blocks",
        parsed_tests.len(),
        parsed_natspec.len()
    );

    let tests_cfg = TestsIntentConfig::default_for_anthropic();
    let natspec_cfg = NatSpecIntentConfig::default_for_anthropic();

    let started = std::time::Instant::now();

    let tests_intents =
        extract_intent_candidates(&parsed_tests, &tests_cfg, provider.clone()).await;
    let natspec_intents = extract_intent_candidates_from_natspec(
        &parsed_natspec,
        &natspec_cfg,
        provider.clone(),
    )
    .await;

    let elapsed = started.elapsed();

    eprintln!(
        "phase4_live: extracted {} tests intents + {} natspec intents in {:.1}s",
        tests_intents.len(),
        natspec_intents.len(),
        elapsed.as_secs_f64()
    );
    for it in &tests_intents {
        eprintln!("  [tests] {}: {}", it.name, it.intent_text);
    }
    for it in &natspec_intents {
        eprintln!(
            "  [natspec/{}/{}] {}: {}",
            it.source_target_kind, it.source_target_name, it.name, it.intent_text
        );
    }

    assert!(
        tests_intents.len() >= 3,
        "SPEC §11.4 floor: expected ≥3 tests intents on vault-4626, got {}",
        tests_intents.len()
    );
    assert!(
        natspec_intents.len() >= 3,
        "SPEC §11.4 floor: expected ≥3 natspec intents on vault-4626, got {}",
        natspec_intents.len()
    );

    // Cost is not gated programmatically — token pricing fluctuates and
    // SPEC §11.4's $1/contract budget is a human-review gate. The
    // detailed token counts are visible via `--nocapture` output. Slice 9
    // closeout records the empirical per-contract cost on the bench.
}
