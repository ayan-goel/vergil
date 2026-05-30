// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");
    bytes32 public constant ADMIN_ROLE = keccak256("ADMIN_ROLE");

    mapping(bytes32 => mapping(address => bool)) internal _roles;

    constructor() {
        _roles[ADMIN_ROLE][msg.sender] = true;
    }

    function hasRole(bytes32 role, address account) external view returns (bool) {
        return _roles[role][account];
    }

    // BUG: no admin check. Any caller can grant any role to anyone.
    function grantRole(bytes32 role, address account) external {
        _roles[role][account] = true;
    }
}
