// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MockToken {
    mapping(address => uint256) public balanceOf;
    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
    }
}

/// Vulnerable: privileged gate reads the caller's spot `balanceOf`.
/// A flash loan inflates the balance for the duration of one
/// transaction, granting the privilege.
contract Target {
    MockToken public immutable token;
    uint256 public constant QUORUM = 1_000_000;
    uint256 public actionsExecuted;

    constructor() {
        token = new MockToken();
    }

    function privileged() external {
        // BUG: spot balance read.
        require(token.balanceOf(msg.sender) >= QUORUM, "Target: not enough");
        actionsExecuted++;
    }
}
