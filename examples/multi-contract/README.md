# examples/multi-contract — vault + token (Phase 4 Slice A4)

Cross-contract verification reference. Two contracts in `src/`:

- `Token.sol` — minimal ERC-20-shaped asset.
- `Vault.sol` — share-token vault that wraps `Token`, issues 1:1 shares.

The canonical cross-contract property: **the vault's recorded total
shares equal the underlying token balance held by the vault**.

## Properties verified

- `check_vault_shares_match_token_balance` — invariant on a sealed
  initial state.
- `check_redeem_preserves_share_asset_match(amount)` — invariant
  preserved through a symbolic redeem.
- `check_vault_total_shares_account_for_holders(other)` — single-holder
  accounting check.

## Usage

```
vergil verify examples/multi-contract
```

Halmos symbolically executes each `check_*` function; the verifier
confirms the cross-contract invariants hold across the symbolic input
space.

## Phase 4 Slice A4

This is the reference scaffold for multi-contract verification — the
N-contract analog of the single-contract `examples/erc20`,
`examples/erc721`, `examples/vault-4626` references. The scaffold
detector reads every `.sol` in `src/`, the signature extractor
collects function signatures from all of them, and the synth prompt
sees the union of available methods.
