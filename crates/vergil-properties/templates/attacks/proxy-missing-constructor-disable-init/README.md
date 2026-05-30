# proxy-missing-constructor-disable-init

Detects upgradeable-pattern implementation contracts whose constructor does not call `_disableInitializers()`. Without this guard the logic contract remains initializable directly (the bypass-the-proxy attack). Close cousin of `init-uninitialized-uups-implementation` (§2.2) — same fixture shape, kept separate per attack-patterns.md §2.4. See `manifest.yaml` and `notes/attack-patterns.md` §2.4.
