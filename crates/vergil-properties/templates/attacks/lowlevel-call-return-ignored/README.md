# lowlevel-call-return-ignored

Detects low-level `call` whose success flag is ignored, causing protocol state
to credit failed external calls. See `manifest.yaml` and
`notes/attack-patterns.md` §11.5.
