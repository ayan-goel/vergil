// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IMulDivLike {
    function mulDivFloor(uint256, uint256, uint256) external pure returns (uint256);
    function mulDivCeil(uint256, uint256, uint256) external pure returns (uint256);
}

contract Check_arith_muldiv_rounding_monotone {
    IMulDivLike public target;

    function check_floor_is_monotone_in_numerator(
        uint256 a,
        uint256 b,
        uint256 c,
        uint256 denom
    ) public view {
        require(denom > 0);
        require(a <= b);
        try target.mulDivFloor(a, c, denom) returns (uint256 r0) {
            try target.mulDivFloor(b, c, denom) returns (uint256 r1) {
                assert(r0 <= r1);
            } catch {}
        } catch {}
    }

    function check_ceil_at_least_floor(
        uint256 a,
        uint256 b,
        uint256 denom
    ) public view {
        require(denom > 0);
        try target.mulDivFloor(a, b, denom) returns (uint256 floor) {
            try target.mulDivCeil(a, b, denom) returns (uint256 ceil) {
                assert(ceil >= floor);
                assert(ceil <= floor + 1);
            } catch {}
        } catch {}
    }
}
