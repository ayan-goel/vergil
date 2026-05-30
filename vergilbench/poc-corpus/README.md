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

## Provenance

V1.5's PoCs are minimal faithful reproductions written by the catalog
author (Claude) from publicly-documented bug patterns. They are NOT
verbatim copies of the original on-chain contracts or third-party
reproductions in DeFiHackLabs / DeFiVulnLabs. Each PoC's README cites
the post-mortem URL and original tx hash where applicable so the
mapping back to the historical exploit is auditable.

**V2 plan:** vendor verbatim reproductions from DeFiHackLabs and
DeFiVulnLabs (both `LICENSE: MIT`) into a `vergilbench/poc-corpus/vendor/`
subtree. The hand-rolled reproductions here serve as the baseline
zero-FN gate and as documentation of which bug shape each historical
incident embodies.

## Why hand-rolled reproductions still count as held-out

The reproductions are written **without referring to the catalog
template's own `vulnerable.sol` fixture**. Each PoC has a realistic
multi-function surface (the historical contract's actual API shape,
simplified) — the template's check must pattern-match through the
declared bindings, NOT through the fixture's narrow shape. A template
that "passes" only because its fixture matches a single function
exactly will fail here.

The Claude-vs-Claude limitation: I authored both the templates AND the
PoCs. An external reproduction (V2) is the cleaner independence test.
But the PoC corpus's broader surface AND its bindings-only template
contract DO surface encoding gaps the integration suite cannot — and
the test driver's pass/fail is itself the ground truth.

## Shipped PoCs (5)

| Incident | Year | Loss USD | Maps to template |
|---|---|---:|---|
| The DAO | 2016 | $60M | `reentrancy-single-function-cei` |
| BeautyChain (BEC) | 2018 | $900M (market cap) | `arith-overflow-underflow-unchecked` |
| Audius | 2022 | $6M | `init-unprotected-initializer` |
| Wormhole | 2022 | $325M | `init-uninitialized-uups-implementation` |
| Cream Finance | 2021 | $130M | `reentrancy-cross-function-state` |

V2 expands the corpus to 10+ incidents, with vendored reproductions.

## Running

```bash
cargo test --package vergilbench --features integration --test poc_corpus
```

A passing run = SPEC §11.2's zero-false-negative gate is met on the
shipped corpus subset.
