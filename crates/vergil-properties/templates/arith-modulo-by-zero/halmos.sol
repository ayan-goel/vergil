// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IModLike {
    function modulo(uint256, uint256) external pure returns (uint256);
}

contract Check_arith_modulo_by_zero {
    IModLike public target;

    function check_mod_by_zero_reverts(uint256 a) public view {
        try target.modulo(a, 0) returns (uint256) {
            assert(false);
        } catch {}
    }
}
