// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Phase 4 Slice A5 — first implementation behind a proxy.
///
/// Storage layout (compile with `solc --storage-layout`):
///   slot 0: uint256 count
///   slot 1: address owner
///
/// Behavioral invariant: count strictly increases on every increment.
contract CounterV1 {
    uint256 public count;
    address public owner;

    constructor(address initialOwner) {
        owner = initialOwner;
    }

    function increment() external {
        unchecked {
            count = count + 1;
        }
    }

    function setOwner(address newOwner) external {
        require(msg.sender == owner, "owner");
        owner = newOwner;
    }
}
