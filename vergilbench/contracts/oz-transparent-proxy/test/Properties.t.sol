// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract, LogicV1} from "../src/Contract.sol";

contract Properties {
    LogicV1 internal logic;
    Contract internal proxy;
    constructor() {
        logic = new LogicV1();
        proxy = new Contract(address(logic), address(this));
    }

    /// A non-admin caller's calls are delegated to the logic and persist.
    function check_nonadmin_call_delegates(uint256 v) external {
        // address(this) is NOT the admin (the admin is an auto-deployed ProxyAdmin),
        // so this call is forwarded to the logic contract.
        LogicV1(address(proxy)).setX(v);
        assert(LogicV1(address(proxy)).x() == v);
    }
}
