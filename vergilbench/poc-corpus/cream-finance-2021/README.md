# Cream Finance (Aug 2021)

~$130M drained via cross-function reentrancy through the AMP token's
ERC-777-style `_callPostTransferHooks`. The attacker borrowed AMP, the
token notified the recipient via callback, and the callback re-entered
a different Cream function before the original borrow's accounting had
closed.

**Maps to:** `reentrancy-cross-function-state`. The template's bug
pattern — two state-changing functions sharing an invariant without a
shared reentrancy lock — is exactly Cream's.

**Post-mortem:** [Cream Finance AMP exploit post-mortem](https://medium.com/cream-finance/c-r-e-a-m-finance-post-mortem-amp-exploit-6ceb20a630c5)
