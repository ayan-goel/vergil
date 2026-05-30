// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean contract: `setProtected` is gated by `onlyOwner`. Same surface as
/// `vulnerable.sol` so the same rendered Halmos template applies to both.
contract Target {
    address public owner;
    uint256 public protectedValue;

    modifier onlyOwner() {
        require(msg.sender == owner, "Target: not owner");
        _;
    }

    constructor() {
        owner = msg.sender;
    }

    function setProtected(uint256 newValue) external onlyOwner {
        protectedValue = newValue;
    }
}
