// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: exchange rate computation reads `externalBalance`
/// (modeled flash-loanable). `inflateExternal` represents the
/// flash-loan-amplified deposit / direct-transfer step.
contract Target {
    uint256 public totalShares;
    uint256 public externalBalance;
    mapping(address => uint256) public sharesOf;

    function deposit(uint256 amount) external returns (uint256) {
        sharesOf[msg.sender] += amount;
        totalShares += amount;
        externalBalance += amount;
        return amount;
    }

    function exchangeRate() external view returns (uint256) {
        if (totalShares == 0) return 1;
        // BUG: rate reads externally-observable balance.
        return externalBalance / totalShares;
    }

    function inflateExternal(uint256 amount) external {
        // Represents a direct transfer / flash-loan-inflated balance.
        externalBalance += amount;
    }
}
