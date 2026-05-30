# init-reinitialization-after-upgrade

Detects upgrade/migration paths that reset the initializer flag, enabling re-initialization with attacker-chosen params. Nexera/AllianceBlock (Aug 2024, ~$440K) is the recurrent real-world reference. The Halmos check initializes legitimately, calls migrate(), then has an Attacker attempt re-initialize — vulnerable's reset lets it through; clean's monotonic flag rejects. See `manifest.yaml` and `notes/attack-patterns.md` §2.5.
