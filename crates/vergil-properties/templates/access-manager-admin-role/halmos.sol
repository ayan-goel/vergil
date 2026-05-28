// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IAccessManagerLike {
    function hasRole(uint64 roleId, address account)
        external view returns (bool isMember, uint32 executionDelay);
}

contract Check_access_manager_admin_role {
    IAccessManagerLike internal manager;
    address internal initialAdmin;

    function check_initial_admin_has_admin_role() external view {
        (bool isMember, uint32 delay) = manager.hasRole(0, initialAdmin);
        assert(isMember);
        assert(delay == 0);
    }
}
