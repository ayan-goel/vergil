// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessManager} from "@openzeppelin/contracts/access/manager/AccessManager.sol";

contract Contract is AccessManager {
    constructor(address admin) AccessManager(admin) {}
}
