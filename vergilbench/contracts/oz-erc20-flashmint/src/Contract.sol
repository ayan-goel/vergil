// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20FlashMint} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20FlashMint.sol";

/// Real-world OZ ERC20FlashMint (EIP-3156 flash loans).
contract Contract is ERC20FlashMint {
    constructor(uint256 s) ERC20("Flash", "FLS") {
        _mint(msg.sender, s);
    }
}
