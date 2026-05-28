// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";

contract Contract {
    using EnumerableMap for EnumerableMap.UintToAddressMap;
    EnumerableMap.UintToAddressMap private _map;

    function set(uint256 k, address v) external returns (bool) { return _map.set(k, v); }
    function get(uint256 k) external view returns (address) { return _map.get(k); }
    function contains(uint256 k) external view returns (bool) { return _map.contains(k); }
    function length() external view returns (uint256) { return _map.length(); }
}
