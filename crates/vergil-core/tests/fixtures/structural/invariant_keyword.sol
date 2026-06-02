// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 5 S1 fixture — invariant-constants Tier A (0.95).
//
// Both `constant` and `immutable` storage variables are baked into
// bytecode at compile/deploy time and cannot be mutated by any later
// transaction. The miner emits one candidate per detected keyword
// declaration.
contract InvariantKeyword {
    uint8 public constant DECIMALS = 18;
    address public immutable OWNER;

    uint256 public counter;

    constructor() {
        OWNER = msg.sender;
    }

    function bump() external {
        counter += 1;
    }
}
