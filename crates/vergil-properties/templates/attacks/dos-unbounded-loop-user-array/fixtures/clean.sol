// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: `process` rejects lengths above the cap.
contract Target {
    uint256 public constant MAX_ITER = 100;
    uint256 public processedTotal;

    function process(uint256 len) external {
        require(len <= MAX_ITER, "Target: exceeds cap");
        processedTotal += len;
    }
}
