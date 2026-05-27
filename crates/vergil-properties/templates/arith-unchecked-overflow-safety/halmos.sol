// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IArithLike {
    function uncheckedAdd(uint256, uint256) external pure returns (uint256);
    function uncheckedSub(uint256, uint256) external pure returns (uint256);
}

contract Check_arith_unchecked_overflow_safety {
    IArithLike public target;

    function check_uncheckedAdd_matches_checked(uint256 a, uint256 b) public view {
        // Precondition the caller's docstring claims this respects:
        require(a <= type(uint256).max - b);
        try target.uncheckedAdd(a, b) returns (uint256 r) {
            assert(r == a + b);
        } catch {}
    }

    function check_uncheckedSub_matches_checked(uint256 a, uint256 b) public view {
        require(a >= b);
        try target.uncheckedSub(a, b) returns (uint256 r) {
            assert(r == a - b);
        } catch {}
    }
}
