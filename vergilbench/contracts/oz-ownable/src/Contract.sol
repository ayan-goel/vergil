// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

contract Contract is Ownable {
    uint256 public value;
    constructor() Ownable(msg.sender) {}
    function setValue(uint256 v) external onlyOwner { value = v; }
}
