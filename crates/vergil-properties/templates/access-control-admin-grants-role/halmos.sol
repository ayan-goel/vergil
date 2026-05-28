// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IAccessControlLike {
    function hasRole(bytes32 role, address account) external view returns (bool);
    function grantRole(bytes32 role, address account) external;
}

contract Check_access_control_admin_grants_role {
    IAccessControlLike internal ac;
    bytes32 internal role;

    function check_admin_can_grant(address account) external {
        ac.grantRole(role, account);
        assert(ac.hasRole(role, account));
    }
}
