// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: deposit and withdraw both update `totalShares` symmetrically,
/// preserving the Σ sharesOf == totalShares invariant.
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
        totalShares -= amount;
    }
}
