// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Minimal hook interface — stands in for IERC777Recipient /
/// IERC721Receiver / IERC1155Receiver. The structural pattern (hook
/// fired before accounting close) is identical across the three.
interface ITokenRecipient {
    function onTokenReceived(uint256 amount) external;
}

/// Vulnerable: `transfer` calls the recipient hook BEFORE finalizing the
/// balance update. A malicious recipient re-enters via the hook, drains
/// twice.
contract Target {
    mapping(address => uint256) public balanceOf;
    uint256 public transferCount;

    function deposit(uint256 amount) external {
        balanceOf[msg.sender] += amount;
    }

    function transfer(address to, uint256 amount) external {
        require(balanceOf[msg.sender] >= amount, "Target: insufficient");
        // BUG: hook fires before balance is finalized.
        ITokenRecipient(to).onTokenReceived(amount);
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        transferCount++;
    }
}
