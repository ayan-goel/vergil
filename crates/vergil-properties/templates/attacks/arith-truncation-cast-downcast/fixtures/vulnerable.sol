// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Target {
    function downcastTo128(uint256 x) external pure returns (uint128) {
        return uint128(x); // BUG: silent truncation for x > 2^128 - 1
    }
}
