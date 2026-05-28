// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// The single admin is enumerable as member 0.
    function check_admin_is_enumerated() external view {
        bytes32 r = c.DEFAULT_ADMIN_ROLE();
        assert(c.getRoleMemberCount(r) == 1);
        assert(c.getRoleMember(r, 0) == address(this));
    }
}
