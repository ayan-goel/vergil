// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal w;
    constructor() { w = new Contract(address(0xBEEF), 1000, 4000, 500); }

    /// The cliff falls within the vesting window [start, end].
    function check_cliff_within_window() external view {
        assert(w.cliff() >= w.start());
        assert(w.cliff() <= w.end());
    }
}
