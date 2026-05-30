// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable token: `mint` lacks an access modifier. Hacken HAI (Oct 2025,
/// ~$170K, BSC/Ethereum) is the canonical real-world instance — a leaked
/// bridge key minted 900M tokens and dumped them.
contract Target {
    address public owner;
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;

    constructor() {
        owner = msg.sender;
    }

    // BUG: missing modifier. Any caller can mint.
    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
        totalSupply += amount;
    }
}
