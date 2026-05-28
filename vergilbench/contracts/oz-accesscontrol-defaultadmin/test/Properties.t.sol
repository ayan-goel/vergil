// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(3 days, address(this)); }

    /// The constructed default admin is reported by defaultAdmin().
    function check_default_admin_set() external view {
        assert(c.defaultAdmin() == address(this));
    }

    /// The configured transfer delay is reported.
    function check_default_admin_delay() external view {
        assert(c.defaultAdminDelay() == 3 days);
    }
}
