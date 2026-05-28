# examples/proxy-upgrade — proxy-upgradeability reference (Phase 4 Slice A5)

Two implementations of the same logical contract — `CounterV1` and
`CounterV2` — share a storage layout for slots 0 and 1. V2 appends a new
slot (slot 2) at the end. Behavior is identical for the common path
(`increment`, `setOwner`); V2 adds `lastIncrementer` tracking.

## Verification mechanisms

Two separate layers cover proxy-upgrade safety:

1. **Storage-layout match** (compile-time, static).
   `vergil_solidity::storage::diff_layouts(v1, v2)` returns an empty
   diff for the shared slots. V2 appends slot 2, which
   `storage::additions_are_appended(&v1_layout, &diff)` confirms is
   safe (strictly higher than every V1 slot).

2. **Behavioral equivalence** (Halmos symbolic execution).
   `vergil verify examples/proxy-upgrade` runs three check_ functions:
   `check_v1_increment_advances_count`, `check_v2_increment_advances_count`,
   `check_v2_setOwner_rejects_non_owner`. All must pass for the
   upgrade to be considered safe.

A real proxy upgrade would gate the deploy on both checks: layout
match (static) + behavioral check (symbolic).

## Verify

```
vergil verify examples/proxy-upgrade
```

Expected: exit 0, all three properties verified.

## Phase 4 Slice A5

Reference scaffold for proxy/upgradeability invariants. Pairs with the
`vergil_solidity::storage::diff_layouts` helper to enable two-tier
upgrade safety: layout stability (slot/type) + behavioral equivalence.
