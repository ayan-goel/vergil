// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 5 S2 fixture — monotonicity (single-polarity).
//
// `count` is only ever incremented (via `++`, `+= 1`). No function
// decrements it. The miner emits a `count >= count_pre` candidate.
contract MonotonicCounter {
    uint256 public count;

    function bump() external {
        count++;
    }

    function bumpBy(uint256 n) external {
        count += n;
    }
}
