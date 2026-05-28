// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";

contract Contract is AccessControl {
    bytes32 public constant MINTER = keccak256("MINTER");
    constructor() { _grantRole(DEFAULT_ADMIN_ROLE, msg.sender); }
}
