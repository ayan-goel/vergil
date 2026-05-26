// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Tiny arithmetic surface used as a target for solc's SMTChecker.
/// Concrete contract (not a library) so SMTChecker actually analyzes the body.
contract SafeMath {
    function addBounded(uint256 a, uint256 b) external pure returns (uint256) {
        require(a <= type(uint128).max);
        require(b <= type(uint128).max);
        uint256 s = a + b;
        assert(s >= a);
        assert(s >= b);
        return s;
    }

    function subBounded(uint256 a, uint256 b) external pure returns (uint256) {
        require(a >= b);
        uint256 d = a - b;
        assert(d <= a);
        return d;
    }
}
