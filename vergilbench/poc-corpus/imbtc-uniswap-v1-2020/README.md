# imBTC / Uniswap V1 (Apr 2020)

~$300k drained via ERC-777 hook reentrancy. imBTC's `transferFrom`
invoked the recipient's `tokensReceived` hook before pool accounting
was finalized; the attacker's recipient re-entered Uniswap V1's
`ethToTokenSwapInput` and got favorable pricing twice from the same
reserve state.

**Maps to:** `reentrancy-callback-token-hook`. The defensive property
— "balance updates committed before the hook fires, OR a shared
nonReentrant lock" — is what imBTC lacked.

**Post-mortem:** [DeFi Rate imBTC analysis](https://defirate.com/imbtc-uniswap-hack/)
