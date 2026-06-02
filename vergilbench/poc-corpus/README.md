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

## Shipped PoCs (11)

### Catalog-author reproductions (10)

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

### Vendored from DeFiVulnLabs (1)

| Source | DVL file | Maps to template |
|---|---|---|
| `dvl-hash-collisions` | `Hash-collisions.sol` | `quirk-abi-encode-packed-collision` |

**Total historical loss represented (catalog-author rows): ~$1.18B.**

All 11 PoCs return `Counterexample` under Halmos against their mapped
templates. SPEC §11.2's zero-false-negative clause is empirically met
on the corpus.

## Provenance

The catalog-author reproductions are minimal faithful Solidity written
by the catalog author from publicly-documented bug patterns. Each cites
its post-mortem URL so the mapping back to the historical exploit is
auditable.

The vendored reproduction (`dvl-hash-collisions/`) uses the verbatim
DVL contract code (MIT licensed) with a thin Vergil adapter to bridge
template-binding rigidity. See "Vendoring experiment" below for what
worked, what didn't, and what blocks further vendoring.

## Vendoring experiment — what worked, what didn't (Nov 2024)

Direct question from a project review: "Why are we using catalog-author
reproductions instead of vendored DeFiVulnLabs / DeFiHackLabs files
directly?" Honest answer documented here.

**8 DeFiVulnLabs files probed for direct vendoring:**

| DVL file | Result | Blocker |
|---|---|---|
| `Hash-collisions.sol` | ✅ vendored via adapter | none — pure-function bug |
| `Visibility.sol` | ❌ not vendored | template binding rigidity (uint256 setter vs address) |
| `Reentrancy.sol` | ❌ blocked | requires `vm.deal` cheatcode for ETH funding |
| `ERC777-reentrancy.sol` | ❌ blocked | requires `vm.etch` to inject ERC-1820 registry bytecode |
| `Overflow.sol` | ❌ blocked | requires `vm.deal` + Solidity 0.7.x (template runs 0.8.20) |
| `SignatureReplay.sol` | ❌ blocked | requires `vm.sign` cheatcode |
| `Returnvalue.sol` | ❌ blocked | uses `vm.createSelectFork("mainnet", N)` — concrete fork replay |
| `Selfdestruct.sol` | ❌ blocked | needs SELFDESTRUCT opcode (Halmos unsupported) + ETH balance |
| `DOS.sol` | ❌ blocked | ETH-dependent (King of Ether shape, pre-funded balances) |
| `empty-loop.sol` | ❌ blocked | uses `vm.startPrank` + payable transfers |

**Hit rate: 1 of 9 attempted, ~11%.**

Pattern: DeFiVulnLabs files are designed for Foundry's `forge test`, not
for Halmos symbolic execution. Most use `vm.*` cheatcodes (`vm.deal`,
`vm.prank`, `vm.sign`, `vm.etch`, `vm.createSelectFork`) which Halmos's
bare scaffold does not model. The one clean win is when the bug is in a
pure function that requires neither state, ETH, nor cheatcodes.

**The Hash-collisions win is real but limited.** The bug behavior
(`abi.encodePacked` collision via `createHash`) lives in the verbatim
DVL contract. A thin `Target` adapter wraps it to expose the template's
required `identify(bytes,bytes)` binding (DVL's signature is
`(string,string)`). Halmos's cex on the adapter traces back to the
vendored `createHash` — so the bug surface is independent of catalog
authorship. But it's one PoC, not ten.

**What unblocks more vendoring:**

1. **Halmos cheatcode emulation** (V2 harness work): if our test
   harness intercepts `vm.deal` / `vm.prank` / `vm.sign` and converts
   them into symbolic-execution-compatible setups, ~6 of the 8 blocked
   DVL files become vendorable. This is a substantial harness
   investment but it has compounding value beyond the PoC corpus.

2. **Template binding flexibility** (V2 template refactor): templates
   that hardcode function names + arg types (`identify(bytes,bytes)`,
   `mint(address,uint256)`, `tick`/`step`/`counter`) need to be
   refactored to accept name + type bindings. Without this, even a
   working harness can't bind a vendored contract that uses
   historical naming.

3. **DeFiHackLabs is harder than DeFiVulnLabs.** DefiHackLabs PoCs
   typically `vm.createSelectFork("mainnet", exploitBlock)` against
   the actually-deployed contracts. They're attack-trace replays,
   not symbolic verifications. Vendoring DeFiHackLabs requires
   extracting the vulnerable contract source separately from the
   replay machinery, which is a manual process per incident.

**For V1.5 the result is:** 1 truly independent vendored PoC + 10
catalog-author reproductions. The vendored count moves from 0/10 to
1/11, with a documented path to scale via V2 harness work.

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

A narrow set of things, honestly stated:

1. **10 of the 50 shipped templates are paired with a real historical
   incident reproduction, and Halmos returns Counterexample on each.**
   The remaining 40 templates have planted-bug fixtures and `provenance.real_world`
   metadata but no PoC mapping — they're not exercised by the
   corpus at all.

2. **Roughly half the PoCs add real state surface; the other half are
   essentially renamed fixtures.** Honest breakdown (by `diff` against
   each template's `vulnerable.sol`):

   - **Genuinely broader** (added state vars, mappings, or functions):
     The DAO (`balanceOf` mapping + `deposit`), Audius (extra
     `quorumThreshold` + `emergencyMode` + setter), Wormhole (extra
     `chainId` + `guardianSetHash`), Cream Finance (extra
     `borrowedBalanceOf` mapping), Cetus (unused `liquidityOf`
     mapping).
   - **Essentially renamed fixtures** (same code structure, different
     contract / function names + different comment strings + different
     revert messages): King of Ether ↔ `dos-push-payment-failure`
     fixture, imBTC ↔ `reentrancy-callback-token-hook` fixture, Hedgey
     ↔ `logic-approval-not-revoked-after-cancel` fixture, Beanstalk ↔
     `flashloan-balance-dependent-state` fixture, BeautyChain (unused
     `balances` mapping, otherwise identical).

   For the renamed-fixture PoCs, the corpus test mostly verifies that
   Halmos handles the rename — which is trivially true, since solc
   compiles the rebrand the same way. The actual independence test
   only applies to the genuinely-broader 5.

   Combined with the binding rigidity gap above (function names
   template-driven, not history-driven), the surface area being
   "tested" by the corpus is narrower than the 10-PoC count suggests.

3. **One false negative on the corpus has been found and fixed in
   the work-to-date** — my first Cetus reproduction used the correct
   overflow threshold (accidentally writing a secure contract). The
   test driver returned `Verified`, the test panicked with FALSE
   NEGATIVE, and I rewrote the PoC to use Cetus's actual buggy
   threshold. The cex appeared. **This is N=1 evidence the
   falsification system functions; it is not evidence the system
   would catch a systematic class of encoding errors.**

4. **The `provenance.real_world` mapping on those 10 templates is
   grounded by a passing test.** Provenance metadata on the other
   40 templates is unverified by the corpus (the manifests reference
   ~30 incidents in total across all 50 templates).

## What the V1.5 corpus does NOT prove

Honestly cataloguing the gaps:

- **That my templates and PoCs are author-independent.** They aren't —
  same author, same week of work. The 10/10 green result is
  consistent with both (a) "the templates correctly encode their bug
  classes" AND (b) "the templates and PoCs share enough latent shape
  that the templates would fail against a truly independent
  reproduction." The Cetus finding is one data point against (b) but
  doesn't refute it. V2's vendored DeFiHackLabs / DeFiVulnLabs
  reproductions are the only way to distinguish.

  Specifically: 9 of 10 PoCs passed on first attempt. Under hypothesis
  (a) that's expected. Under hypothesis (b) it's *also* expected,
  because I'd unconsciously shape the PoC to fit the template I
  already wrote. The data alone can't separate these.

- **That the 40 templates without PoCs catch their named exploits.**
  No PoC, no validation. The integration fixture suite confirms each
  refutes its own planted-bug fixture, but that's a same-author
  test.

- **That a template catches a future variant the catalog author did
  not anticipate.** This is the soundness question, and the V1.5
  corpus does not address it. Only V2's independent reproductions
  begin to; only deployment against real-world unaudited contracts
  (Phase 4+ product surface) fully tests it.

- **That a multi-contract protocol exploit reproduces under the
  bug-shape reduction.** The PoCs are single-contract reductions;
  the actual exploits ran in 5-40 contract protocol contexts the
  reductions don't model.

- **That the catalog catches exploits without a corresponding
  template at all.** Out of scope — the catalog is finite by design.

## TL;DR

The corpus is a **necessary** validation step — it caught one real
encoding error and grounds 10 of 50 templates against historical
incidents — but it's **not a sufficient** one. The 10/10 green
result should be read as "the corpus passed; here is what that
proves and doesn't prove," not as "Vergil's catalog provably catches
$1.18B in historical losses." The right confidence level for the
V1.5 product story is: *templates pair to real bug classes, fire on
broader-state reproductions of those classes, and one self-FN has
already been caught and fixed.* Anything stronger has to wait for V2.
