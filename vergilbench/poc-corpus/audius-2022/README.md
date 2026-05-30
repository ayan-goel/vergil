# Audius (Jul 2022)

~$6M of AUDIO tokens transferred via a malicious governance proposal.
Root cause: the upgradeable Governance contract's `initialize` was not
protected by an `initializer` modifier. The attacker initialized the
proxy after deployment, set themselves as `guardianAddress`, queued a
malicious proposal, and self-executed it.

**Maps to:** `init-unprotected-initializer`. The template's encoding
(an attacker contract attempts a second `initialize()` after the
check contract's legitimate one) catches the missing guard.

**Post-mortem:** [Audius governance takeover post-mortem](https://blog.audius.co/article/audius-governance-takeover-post-mortem-7-23-22)
