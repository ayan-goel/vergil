// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: `withdraw` decrements the per-user balance but forgets to
/// decrement `totalShares` — share-supply drifts upward from the
/// sum-of-balances.
contract Target {
    uint256 public totalShares;
    mapping(address => uint256) public sharesOf;

    function deposit(uint256 amount) external {
        sharesOf[msg.sender] += amount;
        totalShares += amount;
    }

    function withdraw(uint256 amount) external {
        require(sharesOf[msg.sender] >= amount, "Target: insufficient");
        sharesOf[msg.sender] -= amount;
        // BUG: totalShares not decremented.
    }
}
