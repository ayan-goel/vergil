// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessControlDefaultAdminRules} from "@openzeppelin/contracts/access/extensions/AccessControlDefaultAdminRules.sol";

contract Contract is AccessControlDefaultAdminRules {
    constructor(uint48 delay, address admin) AccessControlDefaultAdminRules(delay, admin) {}
}
