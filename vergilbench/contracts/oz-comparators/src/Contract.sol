// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Comparators} from "@openzeppelin/contracts/utils/Comparators.sol";

contract Contract {
    function lt(uint256 a, uint256 b) external pure returns (bool) { return Comparators.lt(a, b); }
    function gt(uint256 a, uint256 b) external pure returns (bool) { return Comparators.gt(a, b); }
}
