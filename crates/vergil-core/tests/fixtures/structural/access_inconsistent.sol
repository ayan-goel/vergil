// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 5 S3 fixture — access-policy negative case.
//
// `deposit` carries `onlyOwner`, but `donate` is permissionless. The
// intersection of writers' modifier sets is empty — no candidate.
contract AccessInconsistent {
    address public owner;
    uint256 public balance;

    constructor() {
        owner = msg.sender;
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }

    function deposit(uint256 amount) external onlyOwner {
        balance += amount;
    }

    function donate(uint256 amount) external {
        balance += amount;
    }
}
