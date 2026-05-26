// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Linear} from "../src/Linear.sol";

contract Properties {
    Linear internal target;
    constructor() { target = new Linear(); }

    function check_doubled_is_monotone(uint64 a, uint64 b) external view {
        if (a <= b) assert(target.doubled(a) <= target.doubled(b));
        else assert(target.doubled(a) > target.doubled(b));
    }
}
