# Property catalog

Vergil ships with 43 Tier-2 property templates — patterns proven
useful on real Solidity contracts, ready for the synthesizer to lift
or adapt. Each template carries an Apache-2.0 license and declares the
storage slots / modifiers it depends on so the manifest validator can
cross-check against solc's layout.

This page is **auto-generated** from
`crates/vergil-properties/templates/*/manifest.yaml`. Re-run
`scripts/gen-catalog-docs.sh` after adding or modifying a template
to refresh.

## Categories

| Category | Templates | Coverage |
|---|---|---|
| ERC-20 | 9 | transfer, transferFrom, approve, sum-of-balances, totalSupply, mint/burn, zero address, allowance handling |
| ERC-721 | 6 | ownerOf invariant, balanceOf, approval flow, mint/burn accounting, unauthorized approve |
| ERC-4626 | 5 | share/asset monotonicity, conservation, preview/actual, no-free-mint, deposit→redeem |
| Access control | 5 | onlyOwner storage, ownership two-step, role membership, ownership transfer, renounce |
| Arithmetic | 5 | overflow safety, division by zero, modulo, mulDiv rounding, identity element |
| AMM | 2 | swap non-empty pool, mint increases supply |
| Lending | 2 | borrow requires collateral, liquidate only undercollateralized |
| State machines | 2 | pause-once-only, reentrancy guard restores |
| Reentrancy | 1 | nonReentrant guard blocks reentry |
| Generic | 1 | zero-amount no-op |
| ERC-20 pausable | 1 | paused blocks transfer |

(Counts approximate — the exact tally lives in
`crates/vergil-properties/templates/`.)

## Browsing the templates

Every template directory follows the same shape:

```
templates/<id>/
  manifest.yaml   # id, description, applies_to, requires, encoding
  halmos.sol      # Halmos check_ functions
  smtchecker.sol  # optional CHC encoding
```

The synthesizer pulls the top-k templates by intent-embedding
similarity (Voyage embeddings; MockEmbedder fallback when no API key).
Pulled templates flow into the prompt as "patterns you might adapt."

## Roadmap to 100 templates

SPEC §3.9 targets a 100-template catalog. Phase 3 shipped 43; the
remaining 57 are split into 4 batches authored against the bench
corpus once it grows:

- Batch 1 (~14): patterns surfacing during the VergilBench full run
  (Phase 4 Slice 12) — gaps the run reveals get prioritized templates
- Batch 2 (~14): governance + multisig + timelock patterns
- Batch 3 (~14): proxy + upgradeability + delegatecall safety
- Batch 4 (~15): oracle + price feed + cross-chain message patterns

Until those land, the synthesizer falls back to "propose from
scratch" — works, but slower convergence than retrieval-assisted.
