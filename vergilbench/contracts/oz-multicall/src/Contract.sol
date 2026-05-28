// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Multicall} from "@openzeppelin/contracts/utils/Multicall.sol";

contract Contract is Multicall {
    uint256 public x;
    uint256 public y;
    function setX(uint256 v) external { x = v; }
    function setY(uint256 v) external { y = v; }
}
