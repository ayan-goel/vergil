// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// sqrt(n) is the integer floor square root: r*r <= n < (r+1)^2.
    function check_sqrt_is_floor(uint256 n) external view {
        require(n <= 1_000_000);
        uint256 r = c.sqrt(n);
        assert(r * r <= n);
        assert((r + 1) * (r + 1) > n);
    }

    /// mulDiv with divisor 1 equals the exact product (within bounds).
    function check_muldiv_identity(uint256 a, uint256 b) external view {
        require(a <= 1e18 && b <= 1e18);
        assert(c.mulDiv(a, b, 1) == a * b);
    }
}
