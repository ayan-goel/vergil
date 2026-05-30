// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: the threshold is `1 << 192` — exactly the largest value that
/// can be shifted left by 64 bits without losing the high 64 bits. Any
/// input that would overflow is rejected before the shift.
contract Target {
    function shiftLeft64(uint256 value) external pure returns (uint256) {
        require(value < (1 << 192), "Target: too large");
        unchecked {
            return value << 64;
        }
    }
}
