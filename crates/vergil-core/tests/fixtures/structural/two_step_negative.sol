// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 5 S5 fixture — two-step negative case.
//
// `setA` writes `a`; `useB` doesn't require `a` (it requires `b` which
// nobody writes). No (F1, F2) pair where F2's require gates on a var
// F1 wrote. No candidate.
contract TwoStepNegative {
    uint256 public a;
    bool public b;

    function setA(uint256 x) external {
        a = x;
    }

    function useB() external view returns (uint256) {
        require(b, "b not set");
        return a;
    }
}
