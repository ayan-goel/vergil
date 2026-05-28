// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// A 6-decimal token (the common USDC-style override).
contract Contract is ERC20 {
    constructor() ERC20("USD6", "U6") { _mint(msg.sender, 1_000_000e6); }
    function decimals() public pure override returns (uint8) { return 6; }
}
