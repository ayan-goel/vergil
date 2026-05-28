// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// A whenNotPaused function reverts while paused.
    function check_bump_blocked_while_paused() external {
        c.pause();
        try c.bump() { assert(false); } catch {}
    }

    /// pause then unpause returns to the unpaused state.
    function check_pause_unpause_round_trip() external {
        c.pause();
        c.unpause();
        assert(!c.paused());
    }
}
