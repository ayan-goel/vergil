// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Pausable} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Pausable.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

/// Real-world OZ ERC20Pausable gated by Ownable.
contract Contract is ERC20Pausable, Ownable {
    constructor(uint256 s) ERC20("Pausable", "PAU") Ownable(msg.sender) {
        _mint(msg.sender, s);
    }
    function pause() external onlyOwner { _pause(); }
    function unpause() external onlyOwner { _unpause(); }
}
