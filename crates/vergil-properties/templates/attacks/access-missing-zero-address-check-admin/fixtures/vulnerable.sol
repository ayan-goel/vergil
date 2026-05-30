// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public admin;
    constructor() { admin = msg.sender; }

    // BUG: no zero-address guard. setAdmin(0) bricks privileged control.
    function setAdmin(address a) external {
        require(msg.sender == admin, "Target: not admin");
        admin = a;
    }
}
