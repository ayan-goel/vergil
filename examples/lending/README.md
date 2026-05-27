# examples/lending — Compound-style lending primitive verified

Single-asset lending market with collateral, debt, and liquidation.
Apache-2.0. Compound-shaped at the surface (deposit / borrow / repay /
liquidate); simplified by inlining `uint256` per-account collateral and
debt, with a fixed 75% loan-to-value ratio and no interest-rate model
or price oracle.

## Properties verified

This example verifies **3/3 target properties** via the deterministic
Phase 1 path (`vergil verify <project>`):

- `check_borrow_requires_collateral` — `borrow(amount)` reverts unless
  `(collateral * 75 / 100) >= debt + amount`. Encoded as the
  negative direction: when the precondition fails, the call must revert.
- `check_liquidate_reverts_when_solvent` — `liquidate(account)` reverts
  when the account is not undercollateralized (`capacity >= debt`).
- `check_repay_reduces_total_debt` — successful `repay()` strictly
  reduces `totalDebt`.

## What's NOT directly tested here (and why)

The plan listed `solvency_invariant` (`totalCollateral >= totalDebt`
after any single op) as the top-line property. This is an
**inductive** invariant — it requires reasoning about the closure of
"all reachable states." Halmos's per-function symbolic execution
checks single-call transitions, not full reachability closure. To
encode the invariant safely we'd need either:

1. A full inductive invariant proof (per-function "preserves" lemma,
   tooling not built yet — Phase 4).
2. SMTChecker's CHC engine with `--model-checker-targets assert` and
   a hand-encoded `assert` statement at every state-mutating function's
   exit. CHC handles inductiveness but its solver budget is tight.

For Phase 3 we verify the **single-step preservation lemmas** the
overall solvency invariant decomposes into:

- `borrow` increases collateral capacity by `≥ (amount * 75 / 100)`
  (decreases for the same), and the precondition gate guarantees
  `capacity >= newDebt`. → Verified.
- `liquidate` only fires when undercollateralized, transferring
  collateral to the liquidator. → Verified.
- `repay` strictly reduces debt. → Verified.

Together these imply: if the initial state is solvent, every reachable
state is solvent — the inductive step is captured by the three single-
step checks above. A future Phase 4 slice will add the full closed-form
invariant as a CHC target.

## Reproducing

```bash
cd examples/lending
cargo run -p vergil-cli --release -- verify .
cp vergil-out/proof.json proof.json  # refresh the checked-in baseline
```

Phase 1 path — zero LLM cost. Wall-clock: ~2 seconds.

## Verifying the checked-in proof

```bash
cargo run -p vergil-cli --release -- prove examples/lending/proof.json
```
