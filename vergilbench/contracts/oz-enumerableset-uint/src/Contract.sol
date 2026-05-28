// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {EnumerableSet} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";

contract Contract {
    using EnumerableSet for EnumerableSet.UintSet;
    EnumerableSet.UintSet private _set;

    function add(uint256 v) external returns (bool) { return _set.add(v); }
    function remove(uint256 v) external returns (bool) { return _set.remove(v); }
    function contains(uint256 v) external view returns (bool) { return _set.contains(v); }
    function length() external view returns (uint256) { return _set.length(); }
}
