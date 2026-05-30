# Hedgey Finance (Apr 2024)

~$44.7M drained ($42.6M Arbitrum + $2.1M Ethereum) via a stale
allowance after claim cancellation. Two bugs combined: anyone could
cancel any claim (missing caller check), AND cancellation marked
inactive without zeroing the per-claim allowance (the recipient
retained transferFrom rights).

**Maps to:** `logic-approval-not-revoked-after-cancel`. The catalog
template was authored specifically for this Hedgey pattern in Phase
1. The joint bug class (also `input-missing-parameter-validation`) is
intentionally covered by two separate templates that both catch
parts of the exploit.

**Post-mortem:** [Hedgey post-mortem](https://medium.com/hedgey-finance/post-mortem-hedgey-finance-token-claim-exploit-april-19-2024-ad36e72e0c33)
