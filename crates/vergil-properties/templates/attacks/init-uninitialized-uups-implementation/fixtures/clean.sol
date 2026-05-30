// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: implementation contract whose constructor calls the equivalent
/// of `_disableInitializers()` — sets the `initialized` flag at deploy
/// time so the logic contract is permanently locked. Calls intended for
/// initialization must land on the proxy (which has its own flag and
/// runs initialize against its delegate-call storage).
contract Target {
    address public owner;
    bool private initialized;

    constructor() {
        // `_disableInitializers()` equivalent: lock the logic-contract
        // initializer at deploy.
        initialized = true;
    }

    function initialize() external {
        require(!initialized, "Target: already initialized");
        initialized = true;
        owner = msg.sender;
    }
}
