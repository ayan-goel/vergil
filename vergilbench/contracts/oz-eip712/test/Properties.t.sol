// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// The domain separator is non-zero and stable across calls.
    function check_domain_separator_nonzero_stable() external view {
        bytes32 s = c.separator();
        assert(s != bytes32(0));
        assert(s == c.separator());
    }
}
