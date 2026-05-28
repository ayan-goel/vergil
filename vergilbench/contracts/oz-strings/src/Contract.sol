// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Strings} from "@openzeppelin/contracts/utils/Strings.sol";

contract Contract {
    function toStr(uint256 n) external pure returns (string memory) { return Strings.toString(n); }
    function eq(string calldata a, string calldata b) external pure returns (bool) { return Strings.equal(a, b); }
}
