// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IMulDivLike {
    function mulDivFloor(uint256 a, uint256 b, uint256 c) external pure returns (uint256);
}

contract Check_arith_rounding_direction_down {
    IMulDivLike public token;

    function check_mulDivFloor_never_exceeds_unrounded(uint256 a, uint256 b, uint256 c) public view {
        require(c > 0);
        require(a <= type(uint128).max && b <= type(uint128).max);
        uint256 lhs = token.mulDivFloor(a, b, c);
        // a * b fits in uint256 when a, b <= 2^128.
        uint256 rhs = (a * b) / c;
        assert(lhs == rhs);
    }
}
