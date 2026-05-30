// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public owner;
    address public implementation;

    constructor() {
        owner = msg.sender;
        implementation = address(uint160(0x1234));
    }

    function upgradeTo(address newImpl) external {
        require(msg.sender == owner, "Target: not owner");
        implementation = newImpl;
    }
}
