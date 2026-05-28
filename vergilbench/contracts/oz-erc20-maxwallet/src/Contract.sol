// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// Anti-whale max-wallet pattern: no account (except via mint/burn) may exceed MAX_WALLET.
contract Contract is ERC20 {
    uint256 public constant MAX_WALLET = 10_000e18;
    constructor() ERC20("MaxWallet", "MW") { _mint(msg.sender, 1_000_000e18); }

    function _update(address from, address to, uint256 value) internal override {
        super._update(from, to, value);
        if (to != address(0)) {
            require(balanceOf(to) <= MAX_WALLET, "max wallet");
        }
    }
}
