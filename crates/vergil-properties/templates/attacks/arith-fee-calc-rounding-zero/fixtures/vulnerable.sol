// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Target {
    uint256 public constant RATE = 5; // 5 basis points = 0.05%
    // BUG: rounds to zero for amount < 10000/5 = 2000.
    function feeFor(uint256 amount) external pure returns (uint256) {
        return (amount * RATE) / 10000;
    }
}
