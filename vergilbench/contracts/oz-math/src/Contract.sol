// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Math} from "@openzeppelin/contracts/utils/math/Math.sol";

contract Contract {
    function sqrt(uint256 n) external pure returns (uint256) { return Math.sqrt(n); }
    function mulDiv(uint256 a, uint256 b, uint256 d) external pure returns (uint256) { return Math.mulDiv(a, b, d); }
    function ceilDiv(uint256 a, uint256 b) external pure returns (uint256) { return Math.ceilDiv(a, b); }
}
