// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: flash-loan dispatch with no upper bound. Attacker can
/// request a loan of arbitrary size.
contract Target {
    uint256 public constant MAX_LOAN = 1_000_000_000;
    uint256 public dispatches;

    function flashLoan(uint256 amount) external returns (uint256) {
        // BUG: amount is not validated against MAX_LOAN.
        dispatches++;
        return amount;
    }
}
