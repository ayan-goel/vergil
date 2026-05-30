# reentrancy-eth-transfer-after-state

Detects the DAO-shape CEI violation specifically at the value-transfer site:
external callback fires before balance decrement, and the decrement is in an
`unchecked` block, so an attacker drains via re-entry. See `manifest.yaml`
and `notes/attack-patterns.md` §4.5.
