// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public owner;
    address public pendingOwner;

    constructor() { owner = msg.sender; }

    // Two-step: transferOwnership only stages the pending owner. The
    // current owner remains in control until acceptOwnership is invoked
    // by the pending owner.
    function transferOwnership(address newOwner) external {
        require(msg.sender == owner, "Target: not owner");
        pendingOwner = newOwner;
    }

    function acceptOwnership() external {
        require(msg.sender == pendingOwner, "Target: not pending");
        owner = pendingOwner;
        pendingOwner = address(0);
    }
}
