// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: `initialize()` has no one-shot guard. Any caller can
/// invoke it at any time, claiming ownership. Audius (Jul 2022) is the
/// canonical instance — a treasury-draining governance proposal followed
/// from the seized initializer.
contract Target {
    address public owner;

    function initialize() external {
        owner = msg.sender;
    }
}
