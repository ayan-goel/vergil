// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Target {
    // BUG: divide before multiply truncates a/c to zero when a < c.
    function compute(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        return (a / c) * b;
    }
}
