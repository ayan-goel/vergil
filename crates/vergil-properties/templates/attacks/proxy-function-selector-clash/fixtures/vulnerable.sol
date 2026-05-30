// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public admin;

    constructor() { admin = msg.sender; }

    // BUG: changeAdmin is exposed without auth (selector-clash class —
    // an impl function with the same selector would route here).
    function changeAdmin(address newAdmin) external {
        admin = newAdmin;
    }
}
