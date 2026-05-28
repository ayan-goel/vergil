// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IMathSqrtLike {
    function sqrt(uint256 n) external pure returns (uint256);
}

contract Check_math_sqrt_is_floor {
    IMathSqrtLike internal helper;

    function check_sqrt_is_floor(uint256 n) external view {
        require(n <= 1_000_000);
        uint256 r = helper.sqrt(n);
        assert(r * r <= n);
        assert((r + 1) * (r + 1) > n);
    }
}
