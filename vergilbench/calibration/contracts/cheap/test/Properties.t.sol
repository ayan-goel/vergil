// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Bounded} from "../src/Bounded.sol";

contract Properties {
    Bounded internal target;
    constructor() { target = new Bounded(); }

    function check_clamped_add_commutative(uint128 a, uint128 b) external view {
        assert(target.clampedAdd(a, b) == target.clampedAdd(b, a));
    }
}
