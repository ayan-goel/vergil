// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IDivLike {
    function divide(uint256, uint256) external pure returns (uint256);
}

contract Check_arith_division_by_zero {
    IDivLike public target;

    function check_div_by_zero_reverts(uint256 a) public view {
        try target.divide(a, 0) returns (uint256) {
            assert(false);
        } catch {}
    }
}
