// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IBitMapLike {
    function get(uint256 i) external view returns (bool);
    function set(uint256 i) external;
    function unset(uint256 i) external;
}

contract Check_struct_bitmaps_set_unset {
    IBitMapLike internal bits;

    function check_set_then_unset(uint256 i) external {
        bits.set(i);
        assert(bits.get(i));
        bits.unset(i);
        assert(!bits.get(i));
    }
}
