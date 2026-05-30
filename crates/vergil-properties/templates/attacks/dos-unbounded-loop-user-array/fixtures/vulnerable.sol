// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: `process` iterates without an upper bound. The length
/// is supplied by the caller — at runtime an array of N entries would
/// be supplied via calldata; we model the bug via the `len` argument
/// directly.
contract Target {
    uint256 public constant MAX_ITER = 100;
    uint256 public processedTotal;

    function process(uint256 len) external {
        // BUG: no upper cap on `len`.
        processedTotal += len;
    }
}
