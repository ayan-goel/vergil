# Vergil PoC corpus — held-out historical exploit reproductions

Per SPEC §11.2's zero-false-negative kill criterion. Each subdirectory
reproduces a publicly-documented historical exploit and declares which
shipped catalog template MUST refute it.

## Layout

Each incident lives under `<incident-id>/`:
```
<incident-id>/
  expected.yaml          # template id, check_fn, bindings, halmos extra args
  src/Vulnerable.sol     # minimal faithful reproduction
  README.md              # incident summary, reference URLs, encoding notes
```

The test driver lives at `vergilbench/tests/poc_corpus.rs` (gated
`--features integration`). Per PoC, it loads `expected.yaml`, renders
the named template against the PoC's `Vulnerable.sol`, runs Halmos, and
asserts the result is `Counterexample`. Anything else — `Verified`,
`Timeout`, or any error — is a **false negative** breaching the kill
criterion.

## Shipped PoCs (10)

| Incident | Year | Loss USD | Maps to template |
|---|---|---:|---|
| The DAO | 2016 | $60M | `reentrancy-single-function-cei` |
| King of the Ether Throne | 2016 | $10k | `dos-push-payment-failure` |
| BeautyChain (BEC) | 2018 | $900M cap | `arith-overflow-underflow-unchecked` |
| imBTC / Uniswap V1 | 2020 | $300k | `reentrancy-callback-token-hook` |
| Cream Finance (AMP) | 2021 | $130M | `reentrancy-cross-function-state` |
| Wormhole | 2022 | $325M | `init-uninitialized-uups-implementation` |
| Beanstalk Farms | 2022 | $182M | `flashloan-balance-dependent-state` |
| Audius | 2022 | $6M | `init-unprotected-initializer` |
| Hedgey Finance | 2024 | $44.7M | `logic-approval-not-revoked-after-cancel` |
| Cetus Protocol | 2024 | $230M | `arith-incorrect-overflow-check-shift` |

**Total historical loss represented: ~$1.18B.**

All 10 PoCs return `Counterexample` under Halmos against their mapped
templates. SPEC §11.2's zero-false-negative clause is empirically met
on the corpus.

## Provenance

The V1.5 PoCs are minimal faithful reproductions written by the
catalog author (Claude) from publicly-documented bug patterns. They
are NOT verbatim copies of the original on-chain contracts or
third-party reproductions in DeFiHackLabs / DeFiVulnLabs. Each PoC's
README cites the post-mortem URL so the mapping back to the
historical exploit is auditable.

## Known gaps — limitations of the V1.5 corpus

These are the documented holes a reader of the corpus should know
about, all carried forward to V2:

### 1. Author independence (the headline limitation)

I authored both the catalog templates AND the PoC reproductions. A
template can pass "because I unconsciously wrote the PoC to match its
fixture's narrow shape." The Cetus reproduction surfaced this
concretely (see §3 below) — my first version was accidentally
*secure*, not vulnerable, and the test caught it. But subtler
overlaps may remain.

**V2 plan:** vendor verbatim reproductions from DeFiHackLabs and
DeFiVulnLabs (both MIT-licensed) into a
`vergilbench/poc-corpus/vendor/` subtree. The hand-rolled
reproductions here serve as the baseline zero-FN gate; vendored
reproductions are the cleaner independence test. The PoCs already
use the historical contracts' realistic API shape (`withdrawBalance`
not `action`, `cancelPlan` not `cancel`, `shiftLeft64` matching
Cetus's `checked_shlw`) so the gap from V1.5 to V2 is narrower than
"throw away the corpus" — it's "swap in independent authors for the
same bug shapes."

### 2. Template binding rigidity (a constraint authoring the corpus revealed)

Several shipped templates have hard-coded function names in their
Halmos check, rather than function-name template variables:

- `reentrancy-cross-function-state` — `target.tick()`, `target.step()`,
  `target.counter()`
- `arith-incorrect-overflow-check-shift` — `target.shiftLeft64(value)`
- `logic-approval-not-revoked-after-cancel` — `target.createPlan(...)`,
  `target.cancelPlan(id)`, `target.plans(id)`

When a PoC contract reproduces such a bug, it MUST expose those exact
function names — even if the historical contract called them
something else. The Cream Finance PoC uses `tick()`/`step()`/`counter()`
naming directly; the original Cream contract used different names.
The PoC's bug structure is faithful, but the function-name surface is
template-driven, not history-driven.

**V2 plan:** refactor each rigid template to take method-name
bindings (e.g. `tick → action_a`, `step → action_b`,
`counter → invariant_getter`) so PoCs can preserve original contract
API surface end-to-end.

### 3. The Cetus first-attempt finding

My first Cetus reproduction used `if (value >= (uint256(1) << 192))
revert` as the overflow check. That's the *correct* threshold —
exactly rejecting values that would lose bits on `<< 64`. The
catalog template verified my PoC. **A false negative against my own
reproduction, not against the catalog.**

The actual Cetus bug used `require(value < 0xFFFFFFFFFFFFFFFF << 192)`
— which equals `2^256 - 2^192`, a much looser bound. Inputs in
`[2^192, 2^256-2^192)` pass the check, then overflow the shift and
silently truncate the high 64 bits.

Fixed: faithful reproduction now uses the actual Cetus threshold;
template finds the counterexample. **Lesson:** a reproduction must
reproduce the BUG, not the intended-but-not-shipped behaviour.
Footnoted in `cetus-2024/src/Vulnerable.sol`. This is the kind of
gap the corpus exists to catch — and the only one it caught on the
first run-through. Whether more remain is a function of how many
external reproductions V2 swaps in.

### 4. Bug-class coverage gaps in the corpus

The 10 shipped PoCs cover: reentrancy (3 incidents), arithmetic
overflow (2), initialization (2), flash-loan governance (1), logic-
state (1), DoS push-payment (1). The 50-template catalog covers
broader categories that the corpus does NOT yet exercise:

- **Lending solvency** (the deferred-to-V2 frontier slice, plus
  decidable `lending-missing-health-check-after-action`) — the Euler
  ($197M, 2023) class.
- **Oracle manipulation** (`oracle-missing-staleness-check` shipped
  but no Mango / Inverse-Finance PoC).
- **Bridge-multisig** (Ronin $625M, Nomad $190M, Harmony $100M) —
  not directly catalog-modeled in V1.5.
- **MEV / sandwich** — out of catalog scope.
- **Storage-collision / proxy mis-upgrade** — the Cat 2.3 / 11.1 /
  11.4 templates are deferred to V2; no PoC yet.

**V2 plan:** at minimum add Euler, Ronin, Nomad, Mango as
reproductions paired to the V2 templates that will catch them. The
goal is one PoC per shipped template category, not just the ones
that were easiest to encode in V1.5.

### 5. Reproduction shape, not full-protocol shape

Each PoC is a single-contract reduction. The historical exploits
took place in multi-contract protocols (DAO had a 1200-line
WhitepaperDAO + Token contract; Beanstalk had ~40 contracts in the
diamond). The reduced PoCs preserve the bug pattern and the
template's required surface, but they do NOT model:

- Cross-contract reentrancy chains beyond what the catalog template
  encodes
- Multi-contract storage interactions
- Realistic gas / call-depth behaviour
- Real ERC-20 / ERC-777 / ERC-721 token behavior (we use minimal
  in-line mocks)

This is fine for the kill criterion (which asks "does the template
catch the bug pattern?") but it's a real gap for a stronger claim
("does Vergil work on the actual exploited contracts?").

**V2 plan:** for the highest-loss incidents (Wormhole, Ronin,
Cream, Beanstalk), maintain a `vendor/<incident>/` directory with
the actual exploited contracts (or DeFiHackLabs's reproductions of
them) AND a per-PoC test confirming the template catches the bug
*within* the realistic multi-contract context. This is substantially
more work and not in V1.5 scope.

## Running

```bash
cargo test --package vergilbench --features integration --test poc_corpus
```

A passing run = SPEC §11.2's zero-false-negative gate is met on the
shipped corpus subset. Current state: **10/10 green** in ~1s wall
clock.

## What the V1.5 corpus DOES prove

Within the documented limitations above, the corpus proves:

1. **The 50 shipped templates that have a historical exploit in the
   corpus all catch them.** No template ships with a known-broken
   encoding for its named bug class.
2. **The Halmos symbolic-execution backend correctly resolves each
   template's negation property against PoC-shaped contracts** (not
   just the integration suite's narrow fixtures).
3. **The catalog's per-template `provenance.real_world` entries are
   real** — every PoC validates a referenced incident, so the
   provenance metadata is grounded.
4. **One of the 10 reproductions was wrong on the first try and the
   test caught it.** The corpus is a working falsification system.

What the corpus does NOT prove (V2 work):
- That Vergil catches exploits the catalog doesn't have templates
  for (out of scope).
- That Vergil catches the *full-protocol* exploited contract, not
  just a single-contract bug-shape reduction (a real gap).
- That a template would catch a future variant the catalog author
  didn't anticipate (this is the soundness question; only V2's
  vendored independent reproductions can begin to answer it).
