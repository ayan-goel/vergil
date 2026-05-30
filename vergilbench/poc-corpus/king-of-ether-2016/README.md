# King of the Ether Throne (Feb 2016)

~$10k locked (small in dollar terms but historically significant —
earliest documented push-payment DoS). When a new monarch claimed
the throne, the contract attempted to compensate the previous king
via a push-payment. If the previous king was a contract whose
receive() reverted, the entire claim transaction failed and the
throne couldn't change hands.

**Maps to:** `dos-push-payment-failure`. The pull-payment pattern
(each recipient withdraws their own credit) is the canonical
mitigation.

**Post-mortem:** [King of the Ether Throne post-mortem](https://www.kingoftheether.com/postmortem.html)
