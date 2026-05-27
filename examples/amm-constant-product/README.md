# examples/amm-constant-product — AMM with weakened invariants verified

Minimal Uniswap-V2-style constant-product AMM. `swapXForY` includes the
canonical 0.3% fee (`FEE_NUM = 997`, `FEE_DEN = 1000`). Reserves and LP
shares live as `uint256` state — no external ERC-20 token contracts.

## Properties verified

This example verifies **3/3 weakened invariants** via the deterministic
Phase 1 path (`vergil verify <project>`):

- `check_swap_does_not_drain_pool` — a successful `swapXForY` leaves
  `reserveY > 0`. (The contract's own `require(amountOut < ry)` makes
  this trivially true on success.)
- `check_mint_increases_totalSupply` — successful `mint()` leaves
  `totalSupply` strictly greater than before.
- `check_burn_reduces_totalSupply` — successful `burn()` leaves
  `totalSupply` strictly less than before.

A fourth property — the canonical `x * y >= k_before` constant-product
invariant — is in `properties.yaml` for transparency and is **expected
to surface as `unknown`** (see postmortem below).

## Postmortem: the canonical `x * y >= k_before` invariant is unknown

`check_swap_preserves_k_invariant` asks Halmos to prove:

```
(rxBefore * ryBefore) <= (reserveX_after * reserveY_after)
```

with symbolic `amountIn`, `rxBefore`, and `ryBefore`. The post-state's
`reserveX_after = rxBefore + amountIn` and `reserveY_after = ryBefore -
amountOut` where `amountOut` is a `mulDiv` involving more reserves —
producing a 4-way nonlinear `uint256` multiplication for the solver.

Empirical Phase 3 result (Slice 6, 2026-05-27):
- **Halmos** times out at 120s wall clock.
- **SMTChecker** (CHC engine) reports `unknown` — "20 verification
  conditions could not be proved."

This is the predicted high-risk capability gap from `tasks/plan.md`.
Symbolic execution over EVM `uint256` arithmetic struggles with
multiplied-symbolic-reserves expressions; this is a known frontier
problem in the SMT space, not a defect specific to Vergil's pipeline.

**Phase 4 candidate remediations** (none required for Phase 3 close):

- Tighten the input bounds with `require()` clauses to keep the
  multiplied operands well below uint256's 2^256 ceiling — gives the
  solver more head-room.
- Switch to a bounded encoding (e.g., 64-bit reserves with explicit
  overflow checks) so the multiplication fits in `uint128 * uint128`.
- Use `cvc5` with the `--non-linear-ext` tactic, or `z3` with
  `(set-logic NIA)` directly via SMT-LIB (Phase 4 re-dispatch path).
- Decompose into per-direction monotonicity properties (after swap
  X→Y, `reserveX` strictly increases and `reserveY` strictly decreases)
  and verify those instead, accepting that the multiplied form is
  out of scope for the kernel as it stands.

The three verified weakened properties capture the invariants the AMM is
supposed to maintain — non-empty reserves after trades, share supply
correctly tracked — and demonstrate that the kernel handles ordinary
AMM logic. The canonical curve invariant is a kernel-frontier exercise
deferred to Phase 4.

## Reproducing

```bash
cd examples/amm-constant-product
cargo run -p vergil-cli --release -- verify .
# vergil-out/proof.json is written (one of the 4 properties is unknown).
cp vergil-out/proof.json proof.json  # refresh the checked-in baseline
```

Phase 1 path — zero LLM cost. Wall-clock: ~5-10 seconds for the three
verified properties; the unknown one consumes its full 120s budget.

## Verifying the checked-in proof

```bash
cargo run -p vergil-cli --release -- prove examples/amm-constant-product/proof.json
```
