// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Minimal ERC-20 with a hard supply cap. Phase 4 Slice A8 bench corpus.
contract CappedToken {
    mapping(address => uint256) public balanceOf;
    uint256 public totalSupply;
    uint256 public immutable cap;

    constructor(uint256 capAmount) {
        cap = capAmount;
    }

    function mint(address to, uint256 amount) external {
        require(totalSupply + amount <= cap, "cap");
        unchecked {
            balanceOf[to] += amount;
            totalSupply += amount;
        }
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        require(balanceOf[msg.sender] >= amount, "balance");
        unchecked {
            balanceOf[msg.sender] -= amount;
            balanceOf[to] += amount;
        }
        return true;
    }
}
