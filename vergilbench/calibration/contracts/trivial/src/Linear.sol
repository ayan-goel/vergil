// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Linear {
    function doubled(uint64 x) external pure returns (uint128) {
        return uint128(x) * 2;
    }
}
