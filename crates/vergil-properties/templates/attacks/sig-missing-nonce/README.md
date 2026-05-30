# sig-missing-nonce

Detects pre-authorized action proofs (signatures, hash commitments) that don't
record consumption, allowing replay. Hash-based encoding substitutes for the
ECDSA flow. See `manifest.yaml` and `notes/attack-patterns.md` §10.1.
