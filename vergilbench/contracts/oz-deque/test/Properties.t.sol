// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// A fresh queue is empty.
    function check_initially_empty() external view {
        assert(c.empty());
        assert(c.length() == 0);
    }

    /// pushBack then front returns the pushed element (single-element queue).
    function check_push_front_roundtrip(bytes32 v) external {
        c.pushBack(v);
        assert(c.length() == 1);
        assert(c.front() == v);
    }
}
