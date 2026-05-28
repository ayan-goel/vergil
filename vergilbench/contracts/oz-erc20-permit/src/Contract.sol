// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Permit} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Permit.sol";

/// Real-world OZ ERC20Permit (EIP-2612 gasless approvals).
contract Contract is ERC20Permit {
    constructor(uint256 s) ERC20("Permit", "PMT") ERC20Permit("Permit") {
        _mint(msg.sender, s);
    }
}
