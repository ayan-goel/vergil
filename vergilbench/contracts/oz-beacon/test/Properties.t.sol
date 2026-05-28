// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract, LogicV1} from "../src/Contract.sol";

contract Properties {
    Contract internal beacon;
    LogicV1 internal logic;
    constructor() {
        logic = new LogicV1();
        beacon = new Contract(address(logic), address(this));
    }

    /// The beacon reports the implementation and owner it was constructed with.
    function check_beacon_implementation_and_owner() external view {
        assert(beacon.implementation() == address(logic));
        assert(beacon.owner() == address(this));
    }
}
