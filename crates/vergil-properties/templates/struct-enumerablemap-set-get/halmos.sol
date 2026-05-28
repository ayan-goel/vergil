// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IMapLike {
    function set(uint256 k, address v) external returns (bool);
    function get(uint256 k) external view returns (address);
    function contains(uint256 k) external view returns (bool);
}

contract Check_struct_enumerablemap_set_get {
    IMapLike internal map;

    function check_set_then_get(uint256 k, address v) external {
        map.set(k, v);
        assert(map.contains(k));
        assert(map.get(k) == v);
    }
}
