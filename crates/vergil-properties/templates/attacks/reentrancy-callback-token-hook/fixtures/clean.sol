// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface ITokenRecipient {
    function onTokenReceived(uint256 amount) external;
}

/// Clean: `transfer` guards itself with a reentrancy lock. The hook
/// can still attempt re-entry, but the lock reverts it.
contract Target {
    mapping(address => uint256) public balanceOf;
    uint256 public transferCount;
    uint256 private _locked;

    modifier nonReentrant() {
        require(_locked == 0, "Target: reentrant");
        _locked = 1;
        _;
        _locked = 0;
    }

    function deposit(uint256 amount) external {
        balanceOf[msg.sender] += amount;
    }

    function transfer(address to, uint256 amount) external nonReentrant {
        require(balanceOf[msg.sender] >= amount, "Target: insufficient");
        ITokenRecipient(to).onTokenReceived(amount);
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        transferCount++;
    }
}
