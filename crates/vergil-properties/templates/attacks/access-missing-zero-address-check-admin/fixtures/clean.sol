// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public admin;
    constructor() { admin = msg.sender; }

    function setAdmin(address a) external {
        require(msg.sender == admin, "Target: not admin");
        require(a != address(0), "Target: zero admin");
        admin = a;
    }
}
