// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ISignedMathLike {
    function max(int256 a, int256 b) external pure returns (int256);
    function min(int256 a, int256 b) external pure returns (int256);
}

contract Check_math_signedmath_max_dominates {
    ISignedMathLike internal helper;

    function check_max_dominates(int256 a, int256 b) external view {
        int256 m = helper.max(a, b);
        assert(m >= a && m >= b);
        assert(m == a || m == b);
        assert(helper.min(a, b) <= m);
    }
}
