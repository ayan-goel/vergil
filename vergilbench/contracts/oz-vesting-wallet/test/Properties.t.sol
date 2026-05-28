// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal w;
    constructor() { w = new Contract(address(0xBEEF), 1000, 4000); }

    /// The vesting schedule end is start + duration.
    function check_end_is_start_plus_duration() external view {
        assert(w.end() == w.start() + w.duration());
    }

    /// Before any funds, releasable ETH is zero.
    function check_initial_releasable_zero() external view {
        assert(w.releasable() == 0);
    }
}
