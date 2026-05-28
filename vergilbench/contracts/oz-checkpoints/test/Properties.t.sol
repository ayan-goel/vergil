// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// After a single push, latest() returns the pushed value and length is 1.
    function check_push_sets_latest(uint48 key, uint208 value) external {
        c.push(key, value);
        assert(c.latest() == value);
        assert(c.length() == 1);
    }
}
