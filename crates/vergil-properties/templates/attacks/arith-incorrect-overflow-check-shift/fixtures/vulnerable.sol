// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: the overflow guard's threshold is wrong. The Cetus
/// production bug (May 22, 2025, ~$223M, Sui) compared the input against
/// `0xFFFFFFFFFFFFFFFF << 192` — a value equal to `2^256 - 2^192`, which
/// is a much looser upper bound than intended. Inputs in
/// `[2^192, 2^256 - 2^192)` pass the check and overflow the subsequent
/// shift, silently losing the high 64 bits.
contract Target {
    function shiftLeft64(uint256 value) external pure returns (uint256) {
        // BUG: wrong threshold constant.
        require(value < 0xFFFFFFFFFFFFFFFF << 192, "Target: too large");
        unchecked {
            return value << 64;
        }
    }
}
