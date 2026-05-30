# access-signature-based-authorization-bypass

Detects ecrecover-based authorization that fails to reject the zero-address recovered signer. When `authorizedSigner` is uninitialized (also zero), an invalid signature (which causes ecrecover to return 0) silently passes the equality check. The Wormhole bridge hack (Feb 2022, ~$326M) is the canonical scale instance of signature-bypass class. See `manifest.yaml` and `notes/attack-patterns.md` §1.11.
