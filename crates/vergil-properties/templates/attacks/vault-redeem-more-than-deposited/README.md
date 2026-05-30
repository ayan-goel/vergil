# vault-redeem-more-than-deposited

Detects vaults where `redeem(shares)` succeeds even when `shares` exceeds the
caller's balance — typically via an `unchecked` block that wraps the
decrement. See `manifest.yaml` and `notes/attack-patterns.md` §5.5.
