# Wormhole (Feb 2022)

~$325M drained via a signature-verification bypass in the Wormhole
bridge. The post-mortem cycle surfaced two distinct architectural
issues in upgradeable bridges, including the canonical
**uninitialized UUPS implementation** anti-pattern: implementation
contracts deployed without `_disableInitializers()` in the
constructor are themselves directly initializable, separate from
their proxy.

**Reproduction scope:** this PoC focuses on the unprotected-UUPS
class (which the Wormhole codebase later mitigated as part of its
hardening). The original signature-verification bypass is in a
different template family and is V2 PoC work.

**Maps to:** `init-uninitialized-uups-implementation`.

**Post-mortem:** [Wormhole Rekt](https://rekt.news/wormhole-rekt/)
