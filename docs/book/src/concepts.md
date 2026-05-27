# Concepts

## Sound vs complete

A verification tool can be **sound** (every accepted result is true —
no false positives) or **complete** (every true result is provable —
no false negatives). Most real-world tools choose one and accept the
other's cost.

Vergil is **sound**. When `vergil verify` reports `verified`, a solver
discharged an SMT-LIB query as UNSAT over the symbolic domain the
encoder explored. The LLM's role is to *propose* properties; the
solver decides. A vacuous spec the LLM hallucinated is caught by the
critique pass (an independent LLM scores vacuity) and the mutation
gate (a real mutation must produce a counterexample) before the
solver ever sees it.

Vergil is **incomplete**. There exist real bugs the kernel will miss:

- Multi-transaction reentrancy chains beyond the loop unroll depth.
- Properties requiring inductive invariants over reachable states
  (the [lending example](https://example.com/vergil/examples/lending)
  decomposes the inductive solvency property into per-call lemmas).
- Nonlinear arithmetic over symbolic `uint256` operands, e.g. the
  AMM `x*y >= k` invariant (documented in `examples/amm-constant-product`).

These are documented frontier problems. Phase 4 candidates include
solver-tactic re-dispatch (z3 NIA, cvc5 `--non-linear-ext`) and
property decomposition prompts.

## The two-axis trust hierarchy

Vergil's defense in depth lives on two orthogonal axes:

1. **Soundness axis** — the SMT solver is the only thing that says
   "verified." Halmos's symbolic execution and SMTChecker's CHC
   encoding both reduce to SMT queries; the solver's UNSAT result is
   the verdict.
2. **Quality axis** — the LLM-proposed spec might be vacuous (true
   for every contract) or off-target (verifying X when the user
   wanted Y). Three guards catch these:
   - **Critique pass** — an independent LLM (different vendor or
     different temperature) scores each candidate on vacuity,
     body_independence, and testability; below-threshold rejects.
   - **Mutation gate** — Gambit mutates the contract; specs that
     still verify against the mutated version are dropped (they
     failed to encode the constraint).
   - **Manifest validation** — every spec declares the storage
     slots and modifiers it depends on; the validator cross-checks
     against solc's storage layout and Slither's detector report.

The output's `quality_metrics` block surfaces all three so the user
can see *how much* defense was active.

## Fuzzing vs formal verification

[**ItyFuzz**](https://github.com/fuzzland/ityfuzz) and Foundry's
`forge invariant` mode are powerful **bug-finding** tools — they
generate random or guided inputs and look for assertion violations.
A passing fuzz run means "no bugs found in N random samples"; it
doesn't mean "no bugs exist."

Vergil's symbolic execution path explores **every** reachable
execution under the bit-precise EVM semantics Halmos encodes (subject
to loop unroll and array length bounds). A passing Halmos verdict on
`check_transfer_preserves_total_supply(address to, uint256 amount)`
means *for every (to, amount) pair the encoded domain permits*,
totalSupply is preserved. That's a real proof — not a sample.

Fuzzing finds bugs faster on novel contracts. Formal verification
proves the absence of bugs on patterns the encoder handles. Vergil
combines them: fuzz-style mutation testing during synthesis pre-flight
(catch vacuity), proof-grade verification for the survivors (catch
correctness).

## Two paths into the pipeline

| Path | Trigger | LLM cost | Determinism |
|---|---|---|---|
| Phase 1 | `vergil verify <project>` with a checked-in `properties.yaml` | $0 | fully deterministic |
| Phase 2 | `vergil verify <project> --intent "..."` | ~$0.50–$10 per run | LLM-guided synthesis |

The Phase 1 path is the right choice once the spec is settled — it's
zero-cost, deterministic, and produces the same proof.json on every
run. The Phase 2 path is the right choice during spec discovery —
the LLM proposes candidates from a natural-language description; you
iterate by tightening the intent rather than hand-coding the spec.
