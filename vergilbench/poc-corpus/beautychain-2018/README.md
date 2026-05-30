# BeautyChain / BEC (Apr 2018) — CVE-2018-10299

`batchTransfer(address[] _receivers, uint256 _value)` computed
`_receivers.length * _value` without overflow check in Solidity 0.4.x.
Crafted inputs wrapped the product to 0, bypassing the
`require(balanceOf >= amount)` precondition, then minted `_value`
tokens to each receiver. BEC's market cap dropped to near-zero
overnight; the loss is conventionally quoted as ~$900M of market cap
evaporated though no protocol funds were directly drained.

**Maps to:** `arith-overflow-underflow-unchecked`. The template's
property `result >= a && result >= b` is refuted by an unchecked
multiplication that wraps.

**Post-mortem:** [PeckShield batchOverflow disclosure](https://medium.com/@peckshield/alert-new-batchoverflow-bug-in-multiple-erc20-smart-contracts-cve-2018-10299-511067db6536)
