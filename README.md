# Vergil

**Formal verification for Solidity — guided by an LLM, but decided by a solver.**

Tell Vergil, in plain English, what should always be true about a contract.
Vergil turns that into formal properties, proves them with a sound SMT solver,
and hands you back either a proof or a runnable test that breaks your contract.

## The problem

Smart contracts are immutable and hold real money, so a single edge-case bug — an
integer overflow, a missing access check, a rounding error that drains a vault — is
often unrecoverable. Two things stand between you and that bug:

- **Tests and fuzzing** check the cases you thought of (or random samples). They can
  show a bug *exists*; they can't show one *doesn't*.
- **Formal verification** proves a property holds for *every* possible input and
  state. It's the real guarantee — but writing the formal specs has traditionally
  needed a verification expert, which is why almost no one does it.

The hard, expensive part of formal verification was never running the solver — it was
**writing the spec**. That's the part Vergil automates.

## What it does, and where it fits

Point Vergil at a Foundry project and describe the invariant you care about:

```bash
vergil verify ./my-token --intent "balances always sum to totalSupply; \
transfers move value without creating or destroying it"
```

Vergil reads the contract, writes the formal properties that capture your intent, and
discharges them with a sound verifier. You get one of:

- ✅ a **proof** the property holds for all inputs (the deciding solver and exact spec recorded),
- ❌ a **counterexample** as a runnable Foundry test you can drop straight into your suite, or
- ❓ an honest **"unknown"** when the property is outside what the solver can decide.

It sits between your test suite and a full manual audit: cheaper and faster than hiring
a verification engineer, far stronger than fuzzing, and honest about what it can't settle.

## Why you can trust the green checkmark

**The LLM only proposes. The solver decides.** A language model writes the candidate
properties — but a property is reported "verified" *only* when a sound SMT solver (via
Halmos or Solidity's SMTChecker) proves it. A hallucinated or wrong guess cannot produce
a false ✅; at worst it yields a property the solver rejects or can't decide. The LLM is
explicitly **not** in the trusted base.

## How it works

A closed loop — counterexample-guided inductive synthesis (CEGIS):

1. **Analyze.** `solc` gives the authoritative storage layout; Slither extracts the call
   graph, modifiers, and inheritance.
2. **Synthesize.** An LLM proposes candidate properties, guided by retrieval over a
   100-template catalog of patterns proven on real contracts.
3. **Critique.** A second, independent LLM rejects vacuous or tautological specs before
   they waste solver time.
4. **Verify.** Halmos (symbolic execution) and Solidity SMTChecker (CHC model checking)
   run as a portfolio backed by Z3 / cvc5; the first definitive verdict wins.
5. **Refine.** On a counterexample, Vergil diagnoses spec-bug vs. real contract-bug, then
   fixes the spec or hands you the failing test. Loop until proven, refuted, or budget spent.

Every verified property in the report names the solver that discharged it and the
static-analysis facts its encoding relied on.

## Using it

Vergil is a Rust workspace, built from source (it's internal / pre-release — not published
to any package registry). From the repo root:

```bash
cargo build --release --bin vergil
./target/release/vergil doctor   # checks solc, halmos, forge, z3, cvc5, slither
```

The LLM-guided path needs provider keys, read from the environment or a local `.env`
(never committed):

```bash
export VERGIL_ANTHROPIC_API_KEY=...   # synthesis
export VERGIL_OPENAI_API_KEY=...      # critique (independent of the synthesizer)
export VOYAGE_API_KEY=...             # retrieval embeddings (optional; local fallback otherwise)
```

Then verify any Foundry project (a directory with `src/` + `foundry.toml`):

```bash
vergil verify ./my-token --intent "..."               # LLM-guided
vergil verify ./my-token                               # deterministic: use a checked-in properties.yaml ($0, no LLM)
vergil prove ./my-token/vergil-out/proof.json          # re-check an existing proof — no LLM, no solver search
```

Results land in `vergil-out/`: `report.md` (human-readable), `proof.json` (machine-checkable —
source hashes, deciding backend, SMT-query hashes), `spec/` (the generated check functions),
and `counterexamples/` (runnable failing tests).

Useful `verify` flags: `--cost-budget <usd>` (per-run cap), `--samples <n>` (synthesis
breadth), `--scaffold <file>` (custom harness for contracts with constructor args),
`--format json`. Exit codes: `0` verified · `1` counterexample · `2` unknown/timeout ·
`3` infrastructure error.

## How well it works

On an internal 100-contract benchmark of real OpenZeppelin-based contracts, Vergil
verifies **~82%** of the targeted safety properties end-to-end — high on the standards
it's built for (ERC-20 / 721 / 1155, access control, vaults, vesting) and honest about
its frontier: it does **not** pretend to prove what SMT can't decide soundly (nonlinear
AMM invariants, elliptic-curve crypto, time-dependent release schedules beyond their
construction-time invariants). The solver decides every result; nothing is rubber-stamped.

## Status & license

Internal / pre-release: built from source, not published, no public packaging. Vergil's
own code and property templates are Apache-2.0 (`LICENSE-APACHE`); bundled benchmark
contracts keep their upstream permissive licenses (see `vergilbench/NOTICE`). Full
internal docs live in `docs/book/` (mdBook).
