// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IAccessControlAdminLike {
    function getRoleAdmin(bytes32 role) external view returns (bytes32);
    function DEFAULT_ADMIN_ROLE() external view returns (bytes32);
}

contract Check_access_control_role_admin_is_default {
    IAccessControlAdminLike internal ac;
    bytes32 internal role;

    function check_role_admin_is_default() external view {
        assert(ac.getRoleAdmin(role) == ac.DEFAULT_ADMIN_ROLE());
    }
}
