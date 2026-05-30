// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public owner;
    bool public destroyed;

    constructor() { owner = msg.sender; }

    // BUG: any caller can mark the implementation destroyed.
    // (Real-world selfdestruct() is the analog; modeled as a flag here
    // because Halmos does not simulate selfdestruct effects directly.)
    function destruct() external {
        destroyed = true;
    }
}
