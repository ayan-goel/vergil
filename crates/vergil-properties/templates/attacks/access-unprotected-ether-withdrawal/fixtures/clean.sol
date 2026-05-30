// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public owner;
    uint256 public internalBalance;

    modifier onlyOwner() {
        require(msg.sender == owner, "Target: not owner");
        _;
    }

    constructor() {
        owner = msg.sender;
        internalBalance = 1_000_000;
    }

    function drain() external onlyOwner {
        internalBalance = 0;
    }
}
