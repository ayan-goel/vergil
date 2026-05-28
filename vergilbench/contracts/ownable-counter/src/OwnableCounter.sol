// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Access-controlled counter — only the owner can increment. Phase 4
/// Slice A8 bench corpus: probes the canonical onlyOwner authorization
/// pattern under symbolic execution.
contract OwnableCounter {
    address public owner;
    uint256 public count;

    constructor(address initialOwner) {
        owner = initialOwner;
    }

    function increment() external {
        require(msg.sender == owner, "owner");
        unchecked {
            count = count + 1;
        }
    }

    function transferOwnership(address newOwner) external {
        require(msg.sender == owner, "owner");
        require(newOwner != address(0), "zero");
        owner = newOwner;
    }
}
