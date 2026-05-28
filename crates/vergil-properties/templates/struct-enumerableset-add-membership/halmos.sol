// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ISetLike {
    function add(address a) external returns (bool);
    function contains(address a) external view returns (bool);
    function length() external view returns (uint256);
}

contract Check_struct_enumerableset_add_membership {
    ISetLike internal set;

    function check_add_makes_present(address a) external {
        require(!set.contains(a));
        uint256 prev = set.length();
        bool added = set.add(a);
        assert(added);
        assert(set.contains(a));
        assert(set.length() == prev + 1);
    }
}
