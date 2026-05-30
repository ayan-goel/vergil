// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Target {
    bool public executed;
    function executeWithDeadline(uint256 deadline) external {
        require(deadline >= block.timestamp, "Target: expired");
        executed = true;
    }
}
