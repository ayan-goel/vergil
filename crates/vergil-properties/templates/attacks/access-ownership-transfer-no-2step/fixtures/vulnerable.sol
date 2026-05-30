// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public owner;

    constructor() { owner = msg.sender; }

    // BUG: one-step transfer. The new owner is set immediately, with no
    // acceptance step. A typo'd / zero-address argument bricks ownership.
    function transferOwnership(address newOwner) external {
        require(msg.sender == owner, "Target: not owner");
        owner = newOwner;
    }
}
