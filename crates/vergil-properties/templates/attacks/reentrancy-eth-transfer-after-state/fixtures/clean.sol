// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: CEI ordering. Balance is decremented BEFORE the value-transfer
/// callback, so any re-entry's balance check fails.
contract Target {
    mapping(address => uint256) public balanceOf;
    uint256 public totalDrained;

    function deposit(uint256 amount) external {
        balanceOf[msg.sender] += amount;
    }

    function withdraw(uint256 amount) external {
        require(balanceOf[msg.sender] >= amount, "Target: insufficient");
        balanceOf[msg.sender] -= amount;
        totalDrained += amount;
        (bool ok, ) = msg.sender.call("");
        require(ok, "Target: callback failed");
    }
}
