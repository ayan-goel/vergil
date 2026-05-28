// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ISafeCastLike {
    function toUint8(uint256 x) external pure returns (uint8);
}

contract Check_math_safecast_overflow_reverts {
    ISafeCastLike internal helper;

    function check_inrange_roundtrips(uint256 x) external view {
        require(x <= 255);
        assert(uint256(helper.toUint8(x)) == x);
    }

    function check_overflow_reverts(uint256 x) external {
        require(x > 255);
        try helper.toUint8(x) { assert(false); } catch {}
    }
}
