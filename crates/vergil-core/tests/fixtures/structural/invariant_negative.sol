// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 5 S1 fixture — invariant-constants negative case.
//
// `value` is written in the constructor AND in `setValue`. The miner
// must NOT classify it as an invariant constant.
contract InvariantNegative {
    uint256 public value;

    constructor(uint256 initial) {
        value = initial;
    }

    function setValue(uint256 v) external {
        value = v;
    }
}
