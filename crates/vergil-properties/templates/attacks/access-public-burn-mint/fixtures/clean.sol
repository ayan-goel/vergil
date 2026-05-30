// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean token: `mint` is gated by an `onlyOwner` check. Same surface as
/// `vulnerable.sol` so the same rendered Halmos template applies to both.
contract Target {
    address public owner;
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;

    modifier onlyOwner() {
        require(msg.sender == owner, "Target: not owner");
        _;
    }

    constructor() {
        owner = msg.sender;
    }

    function mint(address to, uint256 amount) external onlyOwner {
        balanceOf[to] += amount;
        totalSupply += amount;
    }
}
