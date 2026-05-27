# VergilBench (Phase 3 seed corpus)

Multi-contract benchmark for the Vergil pipeline. The Phase 3 deliverable
is the **infrastructure** — corpus layout, runner binary, results
schema, scoreboard template. The full 100-contract sweep is deferred
to Phase 4 launch readiness per `tasks/plan.md` Slice 12.

## Current corpus (7 contracts)

The seed corpus reuses the verified reference contracts already in
`examples/` plus the two OZ-conforming variants from
`tests/kill_criterion_month2/project/src/`:

| Contract | Source | Properties |
|---|---|---|
| `erc20` | examples/erc20 | 2 |
| `erc20-burnable` | kill_criterion_month2 (ERC20Burnable.sol) | 3 |
| `erc20-pausable` | kill_criterion_month2 (ERC20Pausable.sol) | 3 |
| `erc721` | examples/erc721 | 4 |
| `vault-4626` | examples/vault-4626 | 4 |
| `amm-constant-product` | examples/amm-constant-product | 4 (1 unknown) |
| `lending` | examples/lending | 3 |

**23 ground-truth properties total** across these 7 contracts.

## Roadmap to the full 100-contract corpus

Per SPEC §11.3, the bench should cover ~100 contracts across:
- 20 OpenZeppelin contracts (MIT — direct vendoring with NOTICE)
- 30 Certora examples (re-authored or licensed)
- 30 Code4rena / Sherlock contest contracts
- 20 common-pattern contracts (vaults, AMMs, governance, multisigs, oracles)

License diligence on each external source is required before vendoring;
that's the bulk of the Phase 4 work. The bench RUNNER is ready now —
the SWEEP waits until the corpus + ~$50-200 LLM budget is justified by
an external audience.

## Running

```bash
# Run the bench on the current 7-contract seed corpus.
cargo run --release --bin vergilbench -- --corpus vergilbench

# Run on a subset (smoke test):
cargo run --release --bin vergilbench -- --corpus vergilbench --max 3
```

Output: `vergilbench/results/<timestamp>.json` (per-contract details
plus aggregate counters).

Cost: zero with the seed corpus (Phase 1 deterministic Halmos path).
Phase 4 will add a `--intent` mode that drives the CEGIS loop and
costs ~$0.50 per property.
