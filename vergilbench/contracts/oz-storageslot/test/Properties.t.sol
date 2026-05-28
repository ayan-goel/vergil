// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// A value written to an arbitrary slot reads back exactly.
    function check_slot_write_read(bytes32 slot, uint256 v) external {
        c.write(slot, v);
        assert(c.read(slot) == v);
    }
}
