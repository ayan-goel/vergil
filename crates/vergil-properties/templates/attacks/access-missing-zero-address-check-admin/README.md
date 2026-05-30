# access-missing-zero-address-check-admin

Detects admin/owner setters that fail to reject `address(0)`. A zero-address admin permanently disables privileged control — a single typo bricks the contract. Vergil's negation property requires `require(a != address(0))` on every admin-assignment path. See `manifest.yaml` and `notes/attack-patterns.md` §1.7.
