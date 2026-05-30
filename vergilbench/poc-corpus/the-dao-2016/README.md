# The DAO (Jun 2016)

~3.6M ETH (~$60M at the time) drained via reentrancy in `splitDAO` /
`withdrawRewardFor`. The DAO sent the refund via `payOut` (external
call) before the proposer's balance was zeroed; the recipient's
fallback re-entered the same flow with the original balance still in
state, draining repeatedly.

**Maps to:** `reentrancy-single-function-cei`. The template's negation
property — "post-call state mutation count is at most one per outer
call" — is exactly what the DAO violated.

**Post-mortem:** [Understanding the DAO attack (CoinDesk)](https://www.coindesk.com/learn/understanding-the-dao-attack/)
