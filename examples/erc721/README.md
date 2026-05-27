# examples/erc721 — Verified ERC-721 reference

Single-file ERC-721 token contract, verified end-to-end by Vergil via
`vergil verify --intent`. Apache-2.0; semantically equivalent to the
OpenZeppelin ERC-721 for the conformance properties Vergil targets.

## Intent

```
Standard ERC-721 conformance. Every existing token has a non-zero owner
reachable via ownerOf. transferFrom clears the per-token approval.
approve(spender, tokenId) only succeeds for the owner or an operator
approved for all. balanceOf(address(0)) reverts.
```

## Reproducing the verification

```bash
cd examples/erc721
cargo run -p vergil-cli --release -- verify . --intent "$(cat <<'EOF'
Standard ERC-721 conformance. Every existing token has a non-zero owner
reachable via ownerOf. transferFrom clears the per-token approval.
approve(spender, tokenId) only succeeds for the owner or an operator
approved for all. balanceOf(address(0)) reverts.
EOF
)"
# vergil-out/proof.json is written
cp vergil-out/proof.json proof.json  # refresh the checked-in baseline
```

Approximate cost: $1–3 (Sonnet 4.6 synth + GPT-5.5 critique) per run.

## Verifying the checked-in proof

```bash
cargo run -p vergil-cli --release -- prove examples/erc721/proof.json
```

This re-computes the SHA-256 of every source file and confirms it matches
the proof artifact's `source_files[].sha256`, then (Phase 4) re-dispatches
the SMT-LIB query for each `verified_properties[].smt_query_sha256`.

## Properties verified by Vergil

The full kill-criterion ground truth (`tests/kill_criterion_month2/expected/erc721.yaml`)
lists 5 properties; the per-property targeted-intent runner (Slice 1) gets 3/5.
A single broad-intent invocation typically gets 2–3, since one prompt has to
spread its sample budget across the whole property space.

Properties that consistently verify from a broad intent:
- `owner_is_nonzero_for_existing_token` — ownerOf only returns nonzero
  (or reverts on nonexistence)
- `balance_of_zero_reverts` — balanceOf(address(0)) reverts
- `mint_sets_owner_and_increments_balance` — mint correctly updates both
  the owner mapping and the recipient's balance

Properties currently in the Phase 3 stragglers bucket
(both showed `dispatched=0` in the kill-criterion run — GPT-5.5 critique
rejected every candidate):
- `transferFrom_clears_per_token_approval`
- `unauthorized_approve_reverts`

These are tracked in `notes/phase3.md` as further critique tuning work.
Their non-verification doesn't indicate a bug in this contract — both
behaviors are present in the source above and verifiable by hand-written
Halmos specs.
