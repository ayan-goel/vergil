// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Minimal ERC-20-shaped asset the Vault wraps. Designed for
/// `vergil verify`: no external dependencies, deterministic
/// constructor, no transfer hooks. The vault holds the
/// asset balance — the cross-contract invariant pins
/// `vault.totalAssets() == this.balanceOf(address(vault))`.
contract Token {
    mapping(address => uint256) public balanceOf;
    uint256 public totalSupply;

    constructor(uint256 initialSupply, address recipient) {
        balanceOf[recipient] = initialSupply;
        totalSupply = initialSupply;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        require(balanceOf[msg.sender] >= amount, "balance");
        unchecked {
            balanceOf[msg.sender] -= amount;
            balanceOf[to] += amount;
        }
        return true;
    }

    function transferFrom(
        address from,
        address to,
        uint256 amount
    ) external returns (bool) {
        require(balanceOf[from] >= amount, "balance");
        unchecked {
            balanceOf[from] -= amount;
            balanceOf[to] += amount;
        }
        return true;
    }
}
