// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Vault {
    mapping(address => uint256) public balanceOf;
    uint256 public totalDeposited;

    function deposit(uint128 amount) external {
        balanceOf[msg.sender] += amount;
        totalDeposited += amount;
    }

    function withdraw(uint128 amount) external {
        require(balanceOf[msg.sender] >= amount, "insufficient");
        balanceOf[msg.sender] -= amount;
        totalDeposited -= amount;
    }
}
