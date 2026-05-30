# access-unprotected-ether-withdrawal

Detects withdrawal functions that move value without an authorization check. The Parity Multisig (Nov 2017) is the recurrent reference instance. Vergil's negation property requires that every withdrawal either constrain the recipient or gate the caller. The Halmos encoding routes a non-owner Attacker through a `drain()` call and asserts the protected internal balance is unchanged when the attempt succeeds. See `manifest.yaml` for references and `notes/attack-patterns.md` §1.2.
