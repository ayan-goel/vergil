// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// lt and gt agree with the native comparison operators.
    function check_lt_matches_native(uint256 a, uint256 b) external view {
        assert(c.lt(a, b) == (a < b));
        assert(c.gt(a, b) == (a > b));
    }
}
