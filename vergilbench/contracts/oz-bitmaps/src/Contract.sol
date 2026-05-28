// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {BitMaps} from "@openzeppelin/contracts/utils/structs/BitMaps.sol";

contract Contract {
    using BitMaps for BitMaps.BitMap;
    BitMaps.BitMap private _bits;

    function get(uint256 i) external view returns (bool) { return _bits.get(i); }
    function set(uint256 i) external { _bits.set(i); }
    function unset(uint256 i) external { _bits.unset(i); }
}
