// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: rate reads internally-tracked `totalAssets`; the
/// externally-observable balance is decorrelated from the rate.
contract Target {
    uint256 public totalShares;
    uint256 public totalAssets; // internal accounting
    uint256 public externalBalance;
    mapping(address => uint256) public sharesOf;

    function deposit(uint256 amount) external returns (uint256) {
        sharesOf[msg.sender] += amount;
        totalShares += amount;
        totalAssets += amount;
        externalBalance += amount;
        return amount;
    }

    function exchangeRate() external view returns (uint256) {
        if (totalShares == 0) return 1;
        return totalAssets / totalShares;
    }

    function inflateExternal(uint256 amount) external {
        externalBalance += amount; // no effect on rate
    }
}
