// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// The deployer holds DEFAULT_ADMIN_ROLE.
    function check_deployer_is_admin() external view {
        assert(c.hasRole(c.DEFAULT_ADMIN_ROLE(), address(this)));
    }

    /// An admin can grant a role, and the grantee then holds it.
    function check_admin_can_grant(address account) external {
        c.grantRole(c.MINTER(), account);
        assert(c.hasRole(c.MINTER(), account));
    }

    /// The admin of MINTER is DEFAULT_ADMIN_ROLE.
    function check_minter_admin_is_default() external view {
        assert(c.getRoleAdmin(c.MINTER()) == c.DEFAULT_ADMIN_ROLE());
    }
}
