// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

contract Contract is ReentrancyGuard {
    uint256 public calls;
    function guarded() external nonReentrant { calls += 1; }
}
