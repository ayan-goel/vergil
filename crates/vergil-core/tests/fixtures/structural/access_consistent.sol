// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 5 S3 fixture — access-policy positive case.
//
// Both writers of `balance` carry the same `onlyOwner` modifier.
// The miner emits one candidate per (var, shared-modifier) pair.
contract AccessConsistent {
    address public owner;
    uint256 public balance;

    constructor() {
        owner = msg.sender;
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }

    function deposit(uint256 amount) external onlyOwner {
        balance += amount;
    }

    function withdraw(uint256 amount) external onlyOwner {
        balance -= amount;
    }
}
