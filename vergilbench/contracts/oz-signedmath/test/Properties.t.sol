// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// max(a,b) is at least each argument and equals one of them.
    function check_max_dominates(int256 a, int256 b) external view {
        int256 m = c.max(a, b);
        assert(m >= a && m >= b);
        assert(m == a || m == b);
    }

    /// min(a,b) <= max(a,b).
    function check_min_le_max(int256 a, int256 b) external view {
        assert(c.min(a, b) <= c.max(a, b));
    }
}
