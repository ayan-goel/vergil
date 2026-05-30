# vault-donation-exchange-rate-manipulation

Detects ERC-4626 vaults whose exchange-rate computation reads
externally-observable balance (vulnerable) rather than internally-tracked
accounting (clean), so a direct asset transfer shifts the rate. See
`manifest.yaml` and `notes/attack-patterns.md` §5.6.
