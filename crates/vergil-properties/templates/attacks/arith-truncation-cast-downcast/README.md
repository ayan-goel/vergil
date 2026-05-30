# arith-truncation-cast-downcast

Detects `uint256 → uint128/uint64/...` downcasts without bound checks. Silent high-bit truncation corrupts amounts and timestamps. Vergil's negation property requires `(uint256)(uintN)(x) == x` for every x the cast accepts. Sibling of the Cetus BV demo. See `manifest.yaml` and `notes/attack-patterns.md` §3.7.
