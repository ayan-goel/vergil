// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// After set(i), the bit reads true; after unset(i), it reads false.
    function check_set_then_unset(uint256 i) external {
        c.set(i);
        assert(c.get(i));
        c.unset(i);
        assert(!c.get(i));
    }

    /// An untouched bit is false.
    function check_untouched_is_false(uint256 i) external view {
        assert(!c.get(i));
    }
}
