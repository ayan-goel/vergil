// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Counter} from "../src/Counter.sol";

contract Properties {
    Counter internal target;
    constructor() { target = new Counter(); }

    function check_inc_then_dec_restores(address who, uint128 by) external {
        uint256 before = target.count(who);
        target.inc(who, by);
        target.dec(who, by);
        assert(target.count(who) == before);
    }
}
