// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// add/remove round-trips: after remove, the element is absent and length returns to baseline.
    function check_add_remove_roundtrip(uint256 v) external {
        require(!c.contains(v));
        uint256 base = c.length();
        c.add(v);
        assert(c.contains(v) && c.length() == base + 1);
        c.remove(v);
        assert(!c.contains(v) && c.length() == base);
    }
}
