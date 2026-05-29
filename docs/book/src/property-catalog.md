# Property catalog

Vergil ships with **100 property templates** — patterns proven useful on real
Solidity contracts, ready for the synthesizer to lift or adapt. Each template
carries an Apache-2.0 license and declares the storage slots / modifiers it
depends on so the manifest validator can cross-check against solc's layout.

Run `scripts/gen-catalog-docs.sh` to print the live per-category tally; the
authoritative list always lives in `crates/vergil-properties/templates/`.

## Categories

| Category | Templates | Examples |
|---|---|---|
| ERC-20 (+ extensions) | 18 | sum-of-balances, transfer/transferFrom, approve, cap, pause, permit, votes |
| ERC-721 (+ extensions) | 14 | ownerOf invariant, approval flow, mint/burn, enumerable, royalty |
| Access control / ownership | 12 | onlyOwner, two-step transfer, role admin, renounce, AccessControl |
| ERC-4626 vaults | 6 | share/asset monotonicity, conservation, preview vs. actual |
| ERC-1155 | 6 | balance accounting, batch ops, supply tracking, burn |
| Arithmetic / overflow | 6 | overflow safety, division by zero, modulo, mulDiv rounding |
| Utilities | 5 | introspection, context, multicall, common helpers |
| Data structures | 5 | EnumerableSet/Map, bitmaps, deque, checkpoints |
| Proxy / upgradeability | 5 | implementation-slot stability, beacon, clones |
| Math libraries | 4 | bounds, signed math, safe casting |
| Crypto / hashing | 4 | signature-length validation, hash domain separation |
| State machines | 3 | pause-once, reentrancy-guard restore |
| Time / vesting · Lending · Finance · AMM | 8 | schedule = start+duration, collateral checks, fee routing, swap direction |
| Reentrancy · meta-tx · governance · generic | 4 | nonReentrant guard, ERC-2771 context, voting weight, zero-amount no-op |

(Counts are exact as of the last `gen-catalog-docs.sh` run and sum to 100; the
authoritative tally lives in `crates/vergil-properties/templates/`.)

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

## Provenance

The catalog reached its 100-template target in Phase 4, with the second half
authored against the families that actually appear in the VergilBench corpus —
most notably ERC-1155 (previously absent), plus the proxy, data-structure,
crypto, math, and OZ-extension families. Every template is original work
(`provenance.tier: original`), inspired by but not copied from OpenZeppelin and
public spec sets, so the catalog carries a clean Apache-2.0 license throughout.
