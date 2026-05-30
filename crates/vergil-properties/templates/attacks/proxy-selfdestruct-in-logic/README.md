# proxy-selfdestruct-in-logic

Detects reachable selfdestruct (or destruct-equivalent) in upgradeable logic contracts. Modeled here as a `bool destroyed` flag since Halmos does not directly simulate selfdestruct effects; the access-control predicate is the structural defense. Severity context (EIP-6780, Cancun): cross-tx selfdestruct no longer deletes code post-fork. See `manifest.yaml` and `notes/attack-patterns.md` §2.7.
