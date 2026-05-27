# examples/vault-4626 — Verified ERC-4626 vault reference

Minimal ERC-4626 tokenized vault, verified end-to-end by Vergil.
Apache-2.0. Semantically equivalent to the OpenZeppelin ERC-4626 for the
monotonicity, conservation, and round-trip properties Vergil targets;
inlines uint256 asset accounting so Halmos doesn't have to model an
external ERC-20.

## Properties verified

This example verifies **4/5** of the kill-criterion ERC-4626 ground-truth
properties via the deterministic Phase 1 path (`vergil verify <project>`):

- `check_convertToShares_is_monotone` — more asset input never yields fewer shares
- `check_convertToAssets_is_monotone` — more share input never yields fewer assets
- `check_roundtrip_assets_does_not_inflate` — assets → shares → assets never exceeds input
- `check_deposit_increases_totalAssets_by_at_least_paid` — successful deposit credits the vault

## Property documented but not yet verified

`check_deposit_then_redeem_does_not_inflate` — assets → deposit → redeem
should yield ≤ the originally deposited assets. The kill-criterion
per-property targeted-intent runner verifies this on the same contract;
hand-encoding it for the Phase 1 path requires a more delicate setup
(the share-rounding cases need explicit bounds the simple try/catch
template doesn't capture cleanly). Tracked in `notes/phase3.md` for
follow-up.

## Reproducing

```bash
cd examples/vault-4626
cargo run -p vergil-cli --release -- verify .
# vergil-out/proof.json is written
cp vergil-out/proof.json proof.json  # refresh the checked-in baseline
```

Phase 1 path means no LLM cost — pure Halmos symbolic execution.
Wall-clock: ~5 seconds.

## Verifying the checked-in proof

```bash
cargo run -p vergil-cli --release -- prove examples/vault-4626/proof.json
```

Re-computes the SHA-256 of every source file and confirms it matches the
proof artifact's `source_files[].sha256`.

## Intent path

The Phase 2 intent path (`vergil verify . --intent "..."`) is the right
choice for spec discovery from a natural-language description. The
deterministic Phase 1 path used here gives reproducible, zero-cost
verification once the spec is settled.
