# proxy-unprotected-upgrade-function

Detects UUPS proxy implementations whose `_authorizeUpgrade` is missing or empty. Any caller can swap the implementation — the class behind the 2025 automated ERC1967 upgrade campaign. See `manifest.yaml` and `notes/attack-patterns.md` §2.6.
