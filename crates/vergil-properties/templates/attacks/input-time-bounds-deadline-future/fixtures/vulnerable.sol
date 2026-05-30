// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Target {
    bool public executed;
    // BUG: no deadline check.
    function executeWithDeadline(uint256 deadline) external {
        executed = true;
        // ignore deadline
        deadline;
    }
}
