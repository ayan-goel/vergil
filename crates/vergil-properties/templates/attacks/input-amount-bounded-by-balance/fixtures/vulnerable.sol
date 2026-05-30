// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Target {
    mapping(address => uint256) public balanceOf;
    uint256 public totalSupply;
    // BUG: no balance check; the underflow check in 0.8+ would actually
    // revert this, but we model with `unchecked` to simulate the
    // logical attack (where the developer intends a different accounting).
    function withdraw(uint256 amount) external {
        unchecked {
            balanceOf[msg.sender] -= amount;
            totalSupply -= amount;
        }
    }
}
