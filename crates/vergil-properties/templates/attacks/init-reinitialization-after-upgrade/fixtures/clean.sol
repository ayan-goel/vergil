// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public owner;
    bool private initialized;
    uint256 private migrationVersion;

    function initialize() external {
        require(!initialized, "Target: already initialized");
        initialized = true;
        owner = msg.sender;
    }

    // migrate bumps a version counter but never resets the lock.
    function migrate() external {
        require(msg.sender == owner, "Target: not owner");
        migrationVersion += 1;
    }
}
