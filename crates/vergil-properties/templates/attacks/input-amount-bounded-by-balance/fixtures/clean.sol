// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Target {
    mapping(address => uint256) public balanceOf;
    uint256 public totalSupply;
    function withdraw(uint256 amount) external {
        require(amount <= balanceOf[msg.sender], "Target: overdraft");
        balanceOf[msg.sender] -= amount;
        totalSupply -= amount;
    }
}
