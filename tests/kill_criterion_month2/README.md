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

## Status (Phase 2 close, 2026-05-27)

**Kill criterion: PASSED.** Most recent run scored **16/22 = 72.7%**
verified, well above the SPEC §11.2 60% gate. Total cost $12.47; total
wall time ~22 min on Apple Silicon. Result JSON: `results/<timestamp>.json`
(gitignored — re-run locally with `./target/release/kill-criterion`).

Per-contract breakdown from the green run:
* ERC20: 5/6
* ERC20Burnable: 2/3
* ERC20Pausable: 2/3
* ERC721: 3/5
* ERC4626: 4/5

The 6 properties that didn't verify are state-transition / temporal
properties where GPT-5.5's critique pass dropped every synthesized
candidate as low-testability / vacuous before any Halmos call. They
are documented in `notes/phase2.md` as the Phase 3 follow-up targets
(critique threshold tuning + property-aware prompts).

### Layout (final)

`project/src/` holds the 5 Vergil reference token contracts (single
file each, Apache-2.0, no upstream OZ import graph — see NOTICE for
the design choice). `expected/*.yaml` carries the ground-truth set
plus per-property descriptions the runner composes into targeted
intents.

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
