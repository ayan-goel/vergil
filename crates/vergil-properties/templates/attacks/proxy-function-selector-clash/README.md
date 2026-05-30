# proxy-function-selector-clash

Detects transparent-proxy admin selectors that collide with implementation selectors. Modeled as a generic missing-auth-on-admin-function pattern; the "selector clash" framing tells the user what to look for in their proxy / impl pair. See `manifest.yaml` and `notes/attack-patterns.md` §2.9.
