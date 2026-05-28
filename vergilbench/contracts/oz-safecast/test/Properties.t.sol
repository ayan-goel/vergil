// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// Downcasting a value that fits round-trips exactly.
    function check_inrange_roundtrips(uint256 x) external view {
        require(x <= 255);
        assert(uint256(c.toUint8(x)) == x);
    }

    /// Downcasting a value that does not fit reverts.
    function check_overflow_reverts(uint256 x) external {
        require(x > 255);
        try c.toUint8(x) { assert(false); } catch {}
    }
}
