# vault-totalsupply-totalassets-invariant

Detects vault accounting where `totalShares` drifts from the sum of per-user
share balances. Single-user reduction encoding. See `manifest.yaml` and
`notes/attack-patterns.md` §5.3.
