# oracle-missing-staleness-check

Detects oracle consumers that read `(price, updatedAt)` but don't reject stale
prices. See `manifest.yaml` and `notes/attack-patterns.md` §12.2.
