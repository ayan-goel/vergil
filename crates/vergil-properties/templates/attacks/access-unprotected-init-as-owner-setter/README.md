# access-unprotected-init-as-owner-setter

Detects unguarded `initialize()` that assigns owner/admin (the Audius pattern, Jul 2022, ~$6M). Cross-listed with `init-unprotected-initializer` (§2.1) — same flaw, different reporting category. Shares the Phase-1 fixture surface. See `manifest.yaml` and `notes/attack-patterns.md` §1.9.
