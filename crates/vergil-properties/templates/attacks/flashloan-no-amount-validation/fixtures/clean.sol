// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: explicit cap rejects oversized loan requests.
contract Target {
    uint256 public constant MAX_LOAN = 1_000_000_000;
    uint256 public dispatches;

    function flashLoan(uint256 amount) external returns (uint256) {
        require(amount <= MAX_LOAN, "Target: exceeds cap");
        dispatches++;
        return amount;
    }
}
