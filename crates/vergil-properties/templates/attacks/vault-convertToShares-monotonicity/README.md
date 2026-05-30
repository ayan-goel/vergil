# vault-convertToShares-monotonicity

Detects non-monotone `convertToShares(assets)` — depositing more assets must
not yield fewer shares. See `manifest.yaml` and `notes/attack-patterns.md` §5.4.
