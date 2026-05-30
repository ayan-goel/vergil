# access-ownership-transfer-no-2step

Detects single-step ownership transfers — a one-call `transferOwnership` whose new-owner argument is honored immediately. A typo or a wrong address (especially `address(0)`) bricks privileged control irrecoverably. Vergil's negation property requires the two-step pattern (`pendingOwner` + `acceptOwnership`); the Halmos check exercises a single transferOwnership and asserts the original owner is still in control. See `manifest.yaml` and `notes/attack-patterns.md` §1.5.
