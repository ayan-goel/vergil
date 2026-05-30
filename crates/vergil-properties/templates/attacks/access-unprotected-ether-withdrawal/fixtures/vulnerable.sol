// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public owner;
    uint256 public internalBalance;

    constructor() {
        owner = msg.sender;
        internalBalance = 1_000_000;
    }

    // BUG: no auth — anyone can drain the protected balance.
    function drain() external {
        internalBalance = 0;
    }
}
