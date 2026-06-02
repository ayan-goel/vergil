// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 5 S2 fixture — monotonicity negative case.
//
// `value` is both incremented (in `up`) and decremented (in `down`).
// Mixed polarity — not monotonic. The miner must NOT emit a candidate.
contract MonotonicNegative {
    uint256 public value;

    function up(uint256 n) external {
        value += n;
    }

    function down(uint256 n) external {
        value -= n;
    }
}
