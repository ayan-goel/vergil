// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public admin;

    constructor() { admin = msg.sender; }

    function changeAdmin(address newAdmin) external {
        require(msg.sender == admin, "Target: not admin");
        admin = newAdmin;
    }
}
