// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal mgr;
    constructor() { mgr = new Contract(address(this)); }

    /// The initial admin holds the ADMIN_ROLE (roleId 0) with no execution delay.
    function check_initial_admin_has_admin_role() external view {
        (bool isMember, uint32 delay) = mgr.hasRole(0, address(this));
        assert(isMember);
        assert(delay == 0);
    }

    /// A random account does not hold the ADMIN_ROLE.
    function check_other_lacks_admin_role(address other) external view {
        require(other != address(this));
        (bool isMember,) = mgr.hasRole(0, other);
        assert(!isMember);
    }
}
