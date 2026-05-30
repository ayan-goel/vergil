// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: DAO-shape balance accounting. The `withdraw` function
/// dispatches a value-transfer callback BEFORE decrementing the caller's
/// balance, and the decrement is in an `unchecked` block, so an
/// attacker can re-enter through the callback and drain twice — the
/// outer decrement wraps to type(uint256).max instead of reverting.
contract Target {
    mapping(address => uint256) public balanceOf;
    uint256 public totalDrained;

    function deposit(uint256 amount) external {
        balanceOf[msg.sender] += amount;
    }

    function withdraw(uint256 amount) external {
        require(balanceOf[msg.sender] >= amount, "Target: insufficient");
        // BUG: external "value-transfer" callback before balance decrement.
        (bool ok, ) = msg.sender.call("");
        require(ok, "Target: callback failed");
        unchecked { balanceOf[msg.sender] -= amount; }
        totalDrained += amount;
    }
}
