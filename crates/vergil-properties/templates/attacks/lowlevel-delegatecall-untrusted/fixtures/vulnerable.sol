// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: `forward` delegatecalls to any caller-supplied address.
/// Attacker's logic mutates slot 0 (this contract's `owner`).
contract Target {
    address public owner; // slot 0

    constructor() {
        owner = msg.sender;
    }

    function forward(address logic, bytes calldata data) external {
        // BUG: arbitrary delegatecall target with no whitelist.
        (bool ok, ) = logic.delegatecall(data);
        require(ok, "Target: dispatch failed");
    }
}
