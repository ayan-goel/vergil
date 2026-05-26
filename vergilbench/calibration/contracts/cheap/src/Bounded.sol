// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Bounded {
    function clampedAdd(uint128 a, uint128 b) external pure returns (uint256) {
        uint256 s = uint256(a) + uint256(b);
        assert(s >= a);
        assert(s >= b);
        return s;
    }
}
