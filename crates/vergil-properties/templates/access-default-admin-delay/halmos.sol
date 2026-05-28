// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IDefaultAdminRulesLike {
    function defaultAdmin() external view returns (address);
    function defaultAdminDelay() external view returns (uint48);
}

contract Check_access_default_admin_delay {
    IDefaultAdminRulesLike internal ac;
    address internal expectedAdmin;
    uint48 internal expectedDelay;

    function check_default_admin_and_delay() external view {
        assert(ac.defaultAdmin() == expectedAdmin);
        assert(ac.defaultAdminDelay() == expectedDelay);
    }
}
