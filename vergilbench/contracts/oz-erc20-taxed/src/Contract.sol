// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// Fee-on-transfer token over OZ ERC20: a 1% fee routes to the treasury.
contract Contract is ERC20 {
    address public immutable treasury;
    uint256 public constant FEE_BPS = 100; // 1%
    constructor(address treasury_) ERC20("Taxed", "TAX") {
        treasury = treasury_;
        _mint(msg.sender, 1_000_000e18);
    }

    function _update(address from, address to, uint256 value) internal override {
        if (from == address(0) || to == address(0) || from == treasury || to == treasury) {
            super._update(from, to, value);
            return;
        }
        uint256 fee = value * FEE_BPS / 10000;
        super._update(from, treasury, fee);
        super._update(from, to, value - fee);
    }
}
