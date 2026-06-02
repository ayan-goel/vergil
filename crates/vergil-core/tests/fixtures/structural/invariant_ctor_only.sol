// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 5 S1 fixture — invariant-constants Tier B (0.80).
//
// `name` and `symbol` are declared with literal initializers and never
// re-written. `totalSupply` is written only in the constructor, with no
// extractable literal — this is Tier C (low confidence, report-only).
contract InvariantCtorOnly {
    string public name = "Reference";
    string public symbol = "REF";
    uint256 public totalSupply;
    uint256 public counter;

    constructor(uint256 initial) {
        totalSupply = initial;
    }

    function bump() external {
        counter += 1;
    }
}
