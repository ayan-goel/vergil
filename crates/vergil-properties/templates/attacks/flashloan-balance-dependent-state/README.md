# flashloan-balance-dependent-state

Detects permission gates that read `token.balanceOf(caller)` at call time —
flash loans let the attacker hold any spot balance for one transaction. See
`manifest.yaml` and `notes/attack-patterns.md` §13.1.
