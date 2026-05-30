# reentrancy-cross-function-state

Detects reentrancy where the inner re-entry hits a DIFFERENT function from the
outer entry. A per-function guard on the entry function alone does not catch
it; the guard must be SHARED across the functions sharing the invariant. Cream
Finance (Aug 2021) is the standard reference. See `manifest.yaml` and
`notes/attack-patterns.md` §4.2.
