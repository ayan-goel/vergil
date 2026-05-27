# Kill Criterion Test Set (SPEC §11.2 / §9.3)

Frozen reference set Phase 2 must score **≥60%** on. The runner invokes
`vergil verify --intent "..."` against each contract and records how many
of the ground-truth properties verify.

## Layout

```
tests/kill_criterion_month2/
├── README.md                  this file
├── NOTICE                     attribution + license for vendored contracts
├── contracts/                 vendored OpenZeppelin token contracts
│   ├── ERC20.sol
│   ├── ERC20Burnable.sol
│   ├── ERC20Pausable.sol
│   ├── ERC721.sol
│   └── ERC4626.sol
├── expected/                  ground-truth YAML — property name + intent + expected verdict
│   ├── erc20.yaml
│   ├── erc20-burnable.yaml
│   ├── erc20-pausable.yaml
│   ├── erc721.yaml
│   └── erc4626.yaml
├── results/                   per-run output (gitignored except for the green run)
└── runner.rs                  Rust binary that drives the sweep
```

## Status (Phase 2 close)

- **Test-set structure: ready.** Directory layout matches the SPEC; the
  `expected/*.yaml` files name the 22 ground-truth properties Phase 2
  targets (6 ERC20 + 3 burnable + 3 pausable + 5 ERC721 + 5 ERC4626).
- **OpenZeppelin contracts not vendored yet.** Vendoring is gated on the
  `vergil verify --intent` integration (Slice 14 carry-over) actually
  running end-to-end so the sweep produces real numbers, not zeros.
- **Runner not built yet.** Same reason. The interface is documented below
  so the follow-up integration slice can land it cleanly.

## Pass criterion

```
passed = sum(verified_count_per_contract) / sum(ground_truth_count_per_contract)
assert passed >= 0.60
```

22 ground-truth properties total → **≥14 verified to pass.**

## Runner interface (to be implemented)

```rust
struct KillCriterionRun {
    contract: PathBuf,
    intent: String,
    ground_truth: Vec<String>,         // property names expected to verify
    cost_budget_usd: f64,              // per-contract: $5-$10
}
struct KillCriterionAggregate {
    per_contract: Vec<ContractResult>,
    pass_rate: f64,                    // verified / ground_truth
    total_cost_usd: f64,
    aborted_on_budget: bool,
}
```

Per SPEC §11.2's tiered budget table (Slice 13 step 5):
- Per-contract cap: $5–$10
- Aggregate cap: $30 across all 5 contracts
- Aggregate overrun → abort sweep, mark `aborted_on_budget = true`,
  surface in the report

If the run reports <60%, V1 halts per SPEC §11.2 exit test. The pivot
discussion goes into `notes/phase2.md` along with which contracts /
properties failed.
