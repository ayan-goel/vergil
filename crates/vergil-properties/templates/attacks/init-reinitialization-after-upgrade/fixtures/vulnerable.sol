// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public owner;
    bool private initialized;

    function initialize() external {
        require(!initialized, "Target: already initialized");
        initialized = true;
        owner = msg.sender;
    }

    // BUG: migrate() resets the initialized flag, enabling re-init.
    function migrate() external {
        initialized = false;
    }
}
