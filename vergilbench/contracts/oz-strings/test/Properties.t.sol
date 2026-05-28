// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// toString(0) renders the single character "0".
    function check_zero_renders_zero() external view {
        assert(keccak256(bytes(c.toStr(0))) == keccak256(bytes("0")));
    }

    /// Strings.equal is reflexive on a fixed value.
    function check_equal_reflexive() external view {
        assert(c.eq("vergil", "vergil"));
        assert(!c.eq("vergil", "halmos"));
    }
}
