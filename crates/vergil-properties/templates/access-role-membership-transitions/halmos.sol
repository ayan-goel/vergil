// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IAccessControlLike {
    function hasRole(bytes32, address) external view returns (bool);
    function grantRole(bytes32, address) external;
    function revokeRole(bytes32, address) external;
}

contract Check_access_role_membership_transitions {
    IAccessControlLike public ac;

    function check_grant_is_only_promotion_path(
        bytes32 role,
        address account
    ) public {
        require(!ac.hasRole(role, account));
        try ac.grantRole(role, account) {
            assert(ac.hasRole(role, account));
        } catch {}
    }

    function check_revoke_demotes(bytes32 role, address account) public {
        require(ac.hasRole(role, account));
        try ac.revokeRole(role, account) {
            assert(!ac.hasRole(role, account));
        } catch {}
    }
}
