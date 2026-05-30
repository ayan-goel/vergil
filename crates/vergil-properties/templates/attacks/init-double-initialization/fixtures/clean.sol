// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: `initialize()` is guarded by a `bool private initialized` flag
/// set in the first call. The second call reverts; ownership is fixed
/// after the legitimate first init.
contract Target {
    address public owner;
    bool private initialized;

    function initialize() external {
        require(!initialized, "Target: already initialized");
        initialized = true;
        owner = msg.sender;
    }
}
