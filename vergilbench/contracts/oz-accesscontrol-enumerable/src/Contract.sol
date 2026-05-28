// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessControlEnumerable} from "@openzeppelin/contracts/access/extensions/AccessControlEnumerable.sol";

contract Contract is AccessControlEnumerable {
    constructor() { _grantRole(DEFAULT_ADMIN_ROLE, msg.sender); }
}
