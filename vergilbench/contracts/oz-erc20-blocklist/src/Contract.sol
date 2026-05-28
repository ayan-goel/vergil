// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

/// Compliance blocklist pattern over OZ ERC20: blocked accounts cannot move tokens.
contract Contract is ERC20, Ownable {
    mapping(address => bool) public blocked;
    constructor() ERC20("Block", "BLK") Ownable(msg.sender) { _mint(msg.sender, 1e24); }
    function setBlocked(address a, bool v) external onlyOwner { blocked[a] = v; }

    function _update(address from, address to, uint256 value) internal override {
        require(!blocked[from] && !blocked[to], "blocked");
        super._update(from, to, value);
    }
}
