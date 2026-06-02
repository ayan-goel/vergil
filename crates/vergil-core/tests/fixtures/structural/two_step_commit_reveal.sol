// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 5 S5 fixture — two-step positive case.
//
// `commit` sets `committed = true`; `reveal` requires `committed`.
// Two-step pattern: reveal cannot succeed without a prior commit.
contract TwoStepCommitReveal {
    bool public committed;
    uint256 public revealed;

    function commit() external {
        committed = true;
    }

    function reveal(uint256 value) external {
        require(committed, "must commit first");
        revealed = value;
    }
}
