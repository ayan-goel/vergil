// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {SignedMath} from "@openzeppelin/contracts/utils/math/SignedMath.sol";

contract Contract {
    function abs(int256 x) external pure returns (uint256) { return SignedMath.abs(x); }
    function max(int256 a, int256 b) external pure returns (int256) { return SignedMath.max(a, b); }
    function min(int256 a, int256 b) external pure returns (int256) { return SignedMath.min(a, b); }
}
