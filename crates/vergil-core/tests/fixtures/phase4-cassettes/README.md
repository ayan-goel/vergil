# Phase 4 LLM cassettes

This directory holds recorded LLM responses for the snapshot tests in
`tests/phase4_snapshots.rs`. Each cassette captures one LLM exchange
(SHA-256-keyed by canonical request body, per `vergil_llm::mock::MockProvider`).

## Current state

Slice 6 ships the snapshot tests with a **scripted in-process stub**
(`ScriptedExtractor` in `phase4_snapshots.rs`), not disk cassettes.
The stub returns synthetic but schema-conformant JSON keyed off the
prompt content (TEST_TO_PROPERTY vs NATSPEC_TO_PROPERTY).

## Refreshing cassettes (Slice 7+)

When Slice 7 runs the live-LLM exit test against Anthropic, write the
recorded request/response pairs here as `<sha>.json` files:

```json
{ "kind": "completion", "content": "...", "tokens_in": 0, "tokens_out": 0 }
```

Then swap `ScriptedExtractor` for `vergil_llm::mock::MockProvider::new`
pointed at this directory in `phase4_snapshots.rs`. Cassettes are
SHA-keyed, so any change to the prompt template, model, or
TestsIntentConfig / NatSpecIntentConfig invalidates them and they must
be re-recorded.
