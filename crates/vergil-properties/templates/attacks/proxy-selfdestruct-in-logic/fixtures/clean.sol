// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public owner;
    bool public destroyed;

    constructor() { owner = msg.sender; }

    function destruct() external {
        require(msg.sender == owner, "Target: not owner");
        destroyed = true;
    }
}
