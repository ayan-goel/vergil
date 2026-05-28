# Operations runbook

**Audience:** internal operators + the V2 on-call rotation. Covers the
failure modes Phase 4 surfaces in production and the procedures to
unstick them. Cross-references the structured telemetry (Slice B2) so
you know what to grep for.

---

## Diagnosis 101: where to look first

Every Vergil run writes to three places:

1. **`vergil-out/proof.json`** — the result. Empty / partial means the
   run didn't reach the verifier.
2. **`vergil-out/trace/run.jsonl`** — append-only LLM call log. One
   `TraceEvent` per line; bodies under `prompts/<sha>.txt` +
   `responses/<sha>.txt`. Secrets pre-scrubbed.
3. **`--telemetry-json <path>`** (Slice B2) — structured CEGIS event
   stream. Pipe through `jq` to inspect: `jq -c '.' events.jsonl`.

Start at the telemetry stream. The terminal event is
`{"kind":"run_complete", "fields":{"stop_reason":"...", "verified":N}}` —
that one field tells you where the run went sideways.

---

## Stop-reason taxonomy

The `stop_reason` field on the `run_complete` telemetry event is the
fastest triage signal. Values:

| `stop_reason` | What it means | First step |
|---|---|---|
| `verified` | At least one candidate verified. Healthy run. | Confirm `proof.json` has expected properties; close ticket. |
| `max_iterations (N)` | Hit the CEGIS iteration ceiling without verifying. | Check `iteration` count in earlier events. Likely a hard property; consider raising `max_iterations` or splitting the intent. |
| `cost budget $X reached` | Spent `cost_budget_usd` without converging. | Check `synth_sample` events for `tokens_in/out` patterns. Either raise the budget or tighten the intent. |
| `no_definitive_verdict` | Loop ran, dispatched, got only `Unknown` / `Error` verdicts. | Inspect `dispatch_summary` events. Likely a solver timeout — check the halmos stderr in `trace/`. |
| `ambiguous_diagnosis` | Diagnosis pass returned `Ambiguous`. | The intent is genuinely under-specified. Surface to the customer; refuse to guess. |
| `repair_code error: ...` / `refine_spec error: ...` | Refinement LLM call failed. | Check the trace for the LLM error message — usually rate limit, API outage, or context overflow. |

---

## Stuck-run diagnosis

**Symptom:** `vergil verify --intent` hangs past the expected wall
clock.

**Likely causes (in order of frequency):**

1. **Halmos subprocess hung on a solver.** Halmos calls z3 in-process;
   if z3 is grinding on a nonlinear query, it can wedge for minutes.
   `ps -ef | grep halmos` shows the PID. The CEGIS-level budget will
   eventually preempt it via the per-property timeout (default 120s),
   but check `trace/run.jsonl` for the timestamp of the last
   `LlmCall` event — if it's been > 120s, the subprocess is the
   culprit.
2. **LLM API stall.** Anthropic / OpenAI occasionally hangs without
   timing out. Check the trace; if there's no recent `LlmCall` event
   and no telemetry events since the last `synth_sample`, the LLM
   call is the wait. SIGTERM the vergil process and retry; the trace
   resume from where it stopped (request SHAs are stable).
3. **Foundry compile loop.** Forge re-resolves dependencies on every
   compile. If the project has a flaky `lib/` dependency, forge can
   spin. Run `forge build` standalone in the project dir to repro.

**Recovery:**

- SIGTERM (not SIGKILL — let tokio shut down the trace recorder).
- If multiple verticals stuck: `pkill -TERM vergil halmos`.
- Re-run with `--telemetry-json /tmp/last.jsonl` so the next stuck run
  has fresh telemetry.

---

## API-credit exhaustion

**Symptom:** trace shows repeated `Permanent` errors from a provider
with status `payment_required` or `429 Too Many Requests`.

**Phase 4 contract (CLAUDE.md):** API-credit exhaustion **HALTS** the
session. Vergil does not silently retry, does not switch providers as
a workaround, does not keep grinding on offline tasks.

**What you'll see:**

- The run exits with `IntentError::Cegis(...)` carrying the provider
  error verbatim.
- `telemetry-json` has zero events for the offending stage.
- `trace/run.jsonl` has the failed `LlmCall` with the exact provider
  message.

**Recovery:**

1. Read the error to identify the provider (anthropic / openai / voyage).
2. Top up the account at the provider's URL (the error usually
   includes one).
3. Re-run. State is in `vergil-out/`; nothing on disk needs cleanup.

---

## Subprocess crash signatures

| Symptom | Likely subprocess | Fix |
|---|---|---|
| `solc: error: stack too deep` | solc | Compile with `--via-ir`. Vergil's portfolio uses `--via-ir` by default in Phase 4. Confirm the project's `foundry.toml` doesn't override it. |
| `halmos: timeout (X seconds)` | halmos | Raise `PER_HALMOS_SECS` in the runner config; or split the property. Often signals a nonlinear path the symbolic executor can't close. |
| `halmos: assertion failed at ...` | halmos | This is the **expected** counterexample path — not a crash. The CLI surfaces it as `Verdict::Counterexample`. |
| `slither: command not found` | slither | `pipx install slither-analyzer==0.11.0`. Pinned version matters — newer slither has different JSON output. |
| `forge: failed to compile: SPDX license identifier` | forge | The .sol file is missing `// SPDX-License-Identifier: ...`. Add it. |
| `z3: parse error in line X` | z3 (via halmos `--dump-smt-queries`) | The SMT-LIB the backend emitted is malformed. Open a bug — Halmos shouldn't emit invalid SMT. |
| OOM kill (`signal 9`) | usually halmos on a big contract | Halmos's `--smt-limit` defaults to several GB. Either raise the container memory or reduce loop unrolling. |

---

## Solver version bump procedure

When upgrading z3 / cvc5 / halmos / solc:

1. Bump the version in the right place:
   - z3: `Dockerfile` apt install (debian's z3 is recent enough; pin
     by stamping `apt-get install z3=<version>` in the deps stage).
   - cvc5: `Dockerfile` `CVC5_VERSION` arg.
   - solc: `Dockerfile` `SOLC_VERSION` arg + `examples/*/foundry.toml`
     `solc_version`.
   - halmos: `Dockerfile` `pipx install halmos==<version>` + check
     `crates/vergil-solidity/src/halmos.rs` for any CLI flag changes.
2. Rebuild the Docker image. `vergil doctor` inside the container is
   the smoke test.
3. Re-run the kill criterion (`./target/release/kill-criterion`) — the
   22-property baseline catches solver regressions immediately.
4. Re-run the bench seed corpus (`make bench`) — 5 contracts, 17
   properties. Quick sanity at < $1.
5. **Don't** run the full bench sweep on a solver bump unless the kill
   criterion + seed pass. The full sweep is $50-$200; not worth it
   for a smoke test.
6. Update ADR 0003 if the pinned versions changed materially.

---

## Template authoring

The catalog lives under `crates/vergil-properties/templates/`. Each
template is a directory:

```
templates/<template-id>/
  manifest.yaml      # id, description, applicability tags
  halmos.sol         # one Halmos check_ function
  smtchecker.sol     # optional CHC-mode SMTChecker fragment
```

Lint enforced by `cargo test -p vergil-properties`:

- Manifest must include `id`, `description`, `applicability`.
- No GPL / AGPL / BUSL license headers (`vergilbench/contracts/` lint).
- `id` must match the directory name and not collide with siblings.

To add a new template:

1. Create the directory under `templates/<category>-<name>/`.
2. Author `manifest.yaml` + `halmos.sol`.
3. `cargo test -p vergil-properties` — must pass green.
4. Smoke test the template against the example it targets:
   `vergil verify examples/<ref> --intent "..."` — confirm retrieval
   surfaces the new template.

---

## Reading the JSONL telemetry stream

The events are designed to be `jq`-friendly. Common queries:

```bash
# Per-run cost summary
jq -r 'select(.kind=="cost") | "\(.tenant_id)\t\(.fields.usd_estimate)\t\(.fields.wall_clock_ms)ms"' events.jsonl

# Synth samples sorted by tokens
jq -s 'map(select(.kind=="synth_sample")) | sort_by(.fields.tokens_in) | .[-5:]' events.jsonl

# How many candidates the critic dropped per iteration
jq -r 'select(.kind=="critique_summary") | "iter \(.iteration): \(.fields.dropped)/\(.fields.kept+.fields.dropped) dropped"' events.jsonl

# Did anything verify?
jq -r 'select(.kind=="run_complete") | "\(.fields.verified) verified, stop=\(.fields.stop_reason)"' events.jsonl
```

---

## When to escalate

- **Solver bump regressed kill criterion below 20/22** → roll back the
  bump, file a bug with the solver upstream.
- **API outage affecting > 30 min** → switch to manual triage; do not
  retry indefinitely. Phase 3 / 4 rule.
- **Customer reports verified-then-failed counterexample** → trust the
  customer's evidence over our `proof.json`. Re-run with `--solver
  cvc5` (Slice A2) to cross-check. If z3 and cvc5 disagree, surface
  to the kernel team — that's an unsoundness signal, not an ops
  issue.
