// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {EnumerableSet} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";

/// A registry built on OZ EnumerableSet (a very common real-world pattern).
contract Contract {
    using EnumerableSet for EnumerableSet.AddressSet;
    EnumerableSet.AddressSet private members;

    function add(address a) external returns (bool) { return members.add(a); }
    function remove(address a) external returns (bool) { return members.remove(a); }
    function contains(address a) external view returns (bool) { return members.contains(a); }
    function length() external view returns (uint256) { return members.length(); }
}
