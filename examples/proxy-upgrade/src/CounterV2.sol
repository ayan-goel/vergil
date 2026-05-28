// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Phase 4 Slice A5 — second implementation behind the same proxy.
///
/// Storage layout (compile with `solc --storage-layout`):
///   slot 0: uint256 count       (same as V1)
///   slot 1: address owner       (same as V1)
///   slot 2: uint256 lastIncrementer  (appended — proxy-safe)
///
/// Adds `lastIncrementer` tracking. The first two slots are byte-for-byte
/// compatible with V1, so the proxy's existing storage is preserved
/// through the upgrade. V2 adds slot 2 strictly after V1's footprint,
/// which `storage::additions_are_appended` confirms is safe.
contract CounterV2 {
    uint256 public count;
    address public owner;
    uint256 public lastIncrementer;

    constructor(address initialOwner) {
        owner = initialOwner;
    }

    function increment() external {
        unchecked {
            count = count + 1;
        }
        lastIncrementer = uint256(uint160(msg.sender));
    }

    function setOwner(address newOwner) external {
        require(msg.sender == owner, "owner");
        owner = newOwner;
    }
}
