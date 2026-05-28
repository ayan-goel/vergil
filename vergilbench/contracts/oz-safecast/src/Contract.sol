// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {SafeCast} from "@openzeppelin/contracts/utils/math/SafeCast.sol";

contract Contract {
    function toUint8(uint256 x) external pure returns (uint8) { return SafeCast.toUint8(x); }
    function toUint128(uint256 x) external pure returns (uint128) { return SafeCast.toUint128(x); }
}
