// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean contract: addition is performed under Solidity 0.8+ checked
/// arithmetic. Overflow reverts; the assertion in the Halmos check is
/// vacuously satisfied because the test contract never returns from the
/// underlying call.
contract Target {
    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }
}
