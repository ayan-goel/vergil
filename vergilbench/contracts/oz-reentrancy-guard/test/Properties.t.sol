// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// A non-reentrant call to a guarded function succeeds and advances state.
    function check_single_call_succeeds() external {
        uint256 before = c.calls();
        c.guarded();
        assert(c.calls() == before + 1);
    }
}
