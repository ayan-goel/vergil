// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable contract: `add` wraps an `unchecked` block around the addition.
/// On overflow the result silently wraps to a value smaller than either
/// input — exactly the BeautyChain (BEC) `batchOverflow` (Apr 2018) class.
contract Target {
    function add(uint256 a, uint256 b) external pure returns (uint256) {
        unchecked {
            return a + b;
        }
    }
}
