// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MockToken {
    mapping(address => uint256) public balanceOf;
    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
    }
}

/// Clean: privileged gate consults a snapshot the caller had to
/// commit to BEFORE the action — flash-loaned balances at call time
/// don't qualify.
contract Target {
    MockToken public immutable token;
    uint256 public constant QUORUM = 1_000_000;
    uint256 public actionsExecuted;
    mapping(address => uint256) public committedBalance;

    constructor() {
        token = new MockToken();
    }

    /// Snapshot the caller's qualifying balance; only updates allowed
    /// outside a flash-loan window in production, but for the bare
    /// scaffold any caller must explicitly commit first.
    function commit() external {
        committedBalance[msg.sender] = token.balanceOf(msg.sender);
    }

    function privileged() external {
        require(committedBalance[msg.sender] >= QUORUM, "Target: not committed");
        actionsExecuted++;
    }
}
