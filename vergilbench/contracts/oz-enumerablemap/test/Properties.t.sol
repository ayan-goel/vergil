// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// After set(k,v), the key is present and reads back v.
    function check_set_then_get(uint256 k, address v) external {
        c.set(k, v);
        assert(c.contains(k));
        assert(c.get(k) == v);
    }

    /// Re-setting an existing key does not grow the map.
    function check_overwrite_no_growth(uint256 k, address v1, address v2) external {
        c.set(k, v1);
        uint256 mid = c.length();
        c.set(k, v2);
        assert(c.length() == mid);
        assert(c.get(k) == v2);
    }
}
