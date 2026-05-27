# Architecture

Vergil is a Rust workspace with one CLI binary and seven library
crates. The pipeline is a closed loop — synthesize, critique, mutate,
validate, dispatch, diagnose, refine — designed so the LLM proposes
specs but never decides correctness.

## Pipeline

```
+--------+    +---------+    +----------+    +---------+    +---------+
| Intent | -> |Synthesis| -> | Critique | -> |Mutation | -> |Dispatch |
+--------+    +---------+    +----------+    +---------+    +---------+
                  ^                                              |
                  |              +---------+                     |
                  +------------- |Diagnosis| <-------------------+
                  |              +---------+
                  |                   |
                  +-- Refinement -----+
```

| Stage | Crate / module | Responsibility |
|---|---|---|
| Intent | `vergil-cli::commands::verify::run_with_intent` | Build provider bundle, render scaffold, kick off the loop |
| Synthesis | `vergil-core::synthesis` | Sample K candidates from the synthesizer LLM with retrieval context |
| Critique | `vergil-core::critique` | Independent LLM scores vacuity/body_independence/testability; reject below threshold |
| Mutation gate | `vergil-mutation` (`vergil-core::cegis`) | Optional Gambit-driven pre-flight; drop specs that pass against a mutated contract |
| Dispatch | `vergil-core::portfolio` → `vergil-solidity` | Concurrent Halmos + SMTChecker; first definitive verdict wins |
| Diagnosis | `vergil-core::diagnosis` | When a CEX surfaces, classify as code-bug vs spec-bug |
| Refinement | `vergil-core::refinement` | Auto-patch the spec (spec-bug) or surface a code-repair plan (code-bug) |

## Crate layout

```
crates/
  vergil-core/       # CEGIS loop, portfolio, critique, diagnosis, refinement
  vergil-llm/        # Provider abstraction (Anthropic, OpenAI, Voyage); trace recorder
  vergil-solidity/   # Halmos / SMTChecker / Slither / forge wrappers
  vergil-properties/ # Template catalog, manifest validation, embedding retrieval
  vergil-mutation/   # Gambit-driven mutation testing
  vergil-proof/      # proof.json schema + serializer
  vergil-cli/        # `vergil` binary (commands::*); also hosts kill_criterion + vergilbench
vergilbench/         # Bench runner + seed corpus
examples/            # Verified reference contracts (erc20, erc721, vault-4626, amm, lending)
tests/kill_criterion_month2/  # SPEC §11.2 kill-criterion ground truth
```

## Trust boundary

The trusted base is exactly:

- The Rust workspace under `crates/` (where Vergil lives)
- The external tools it shells out to (`solc`, `halmos`, `forge`,
  `slither`, `z3`, `cvc5`)
- The libraries those tools link

The LLM is **not** in the trusted base. Anthropic, OpenAI, and Voyage
APIs influence which candidates the synthesizer proposes and how
critique scores them, but they never decide whether a property holds.
A property is "verified" only when the SMT solver returns UNSAT on the
encoded query.

## Proof artifact (`proof.json`)

Every successful run writes `vergil-out/proof.json` with:

```json
{
  "vergil_version": "0.0.1",
  "schema_version": 1,
  "run": { "run_id": "...", "intent": "...", "started_at": "..." },
  "toolchain": { "solc": "0.8.20", "halmos": "0.3.3", ... },
  "source_files": [{ "path": "src/Token.sol", "sha256": "..." }],
  "verified_properties": [
    {
      "name": "check_transfer_preserves_total_supply",
      "backend": "halmos",
      "spec_sha256": "...",
      "smt_query_sha256": "..." | null,
      "wall_clock_ms": 30,
      ...
    }
  ],
  "quality_metrics": {
    "critique_pass_rate": 0.83,
    "mutation_coverage_min": null,
    "mutation_testing_enabled": false
  },
  "cost": { "tokens_in": 0, "tokens_out": 0, "usd_estimate": 0.0, "wall_clock_ms": 80 }
}
```

`vergil prove <path>` re-computes the source SHAs and (Phase 4)
re-dispatches the SMT query for each verified property — a fresh
proof without re-running Halmos.

## CEGIS loop budgets

The loop has hard caps to prevent runaway cost (SPEC §3.1):

- Max iterations: 10 (default; CLI uses 3)
- Per-run cost budget: $10 in CLI mode, $200 in production, $2 per
  property in the kill criterion sweep
- Per-property wall clock: 120s for Halmos, same for SMTChecker

When any cap is hit, the loop returns with `stop_reason` set so the
caller sees exactly why (`cost budget`, `max iterations`, etc.).
