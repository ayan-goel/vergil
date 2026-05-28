// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// Adding a new element makes it present and grows the set by one.
    function check_add_makes_present(address a) external {
        require(!c.contains(a));
        uint256 before = c.length();
        bool added = c.add(a);
        assert(added);
        assert(c.contains(a));
        assert(c.length() == before + 1);
    }

    /// Adding an existing element is a no-op returning false.
    function check_double_add_is_noop(address a) external {
        c.add(a);
        uint256 mid = c.length();
        bool again = c.add(a);
        assert(!again);
        assert(c.length() == mid);
    }
}
