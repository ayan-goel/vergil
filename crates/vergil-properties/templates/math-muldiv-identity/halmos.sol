// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IMathMulDivLike {
    function mulDiv(uint256 a, uint256 b, uint256 d) external pure returns (uint256);
}

contract Check_math_muldiv_identity {
    IMathMulDivLike internal helper;

    function check_muldiv_identity(uint256 a, uint256 b) external view {
        require(a <= 1e18 && b <= 1e18);
        assert(helper.mulDiv(a, b, 1) == a * b);
    }
}
