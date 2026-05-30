// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public owner;
    address public implementation;

    constructor() {
        owner = msg.sender;
        implementation = address(uint160(0x1234));
    }

    // BUG: no auth — any caller can upgrade.
    function upgradeTo(address newImpl) external {
        implementation = newImpl;
    }
}
