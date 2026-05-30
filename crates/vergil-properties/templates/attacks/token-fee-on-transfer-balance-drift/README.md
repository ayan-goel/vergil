# token-fee-on-transfer-balance-drift

Detects protocols that pull tokens via `transferFrom` and credit the requested
amount rather than the actually-received amount, breaking accounting for
fee-on-transfer tokens. See `manifest.yaml` and `notes/attack-patterns.md` §9.1.
