// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: `forward` only delegatecalls to a whitelisted logic address.
contract Target {
    address public owner; // slot 0
    address public whitelistedLogic; // slot 1

    constructor() {
        owner = msg.sender;
        // Whitelist is set to an inert dummy at deploy time. In production
        // the whitelist would be an upgradeable-proxy logic contract.
        whitelistedLogic = address(0x1);
    }

    function forward(address logic, bytes calldata data) external {
        require(logic == whitelistedLogic, "Target: not whitelisted");
        (bool ok, ) = logic.delegatecall(data);
        require(ok, "Target: dispatch failed");
    }
}
