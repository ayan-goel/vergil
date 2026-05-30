# sig-ecrecover-zero-address-bypass

Detects auth comparisons that fail to reject `address(0)`. When the authorized
signer is uninitialized (also `address(0)`), forged inputs pass. See
`manifest.yaml` and `notes/attack-patterns.md` §10.3.
