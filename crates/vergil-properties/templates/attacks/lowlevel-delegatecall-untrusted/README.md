# lowlevel-delegatecall-untrusted

Detects `delegatecall(target, data)` where `target` is attacker-controlled and
no whitelist gates the dispatch. Parity Wallet (Jul 2017) class. See
`manifest.yaml` and `notes/attack-patterns.md` §11.3.
