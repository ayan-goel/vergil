// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface ITokenRecipient {
    function onTokenReceived(uint256 amount) external;
}

/// imBTC / Uniswap V1 ERC-777 reentrancy (Apr 2020).
///
/// Reproduction note: imBTC's `transferFrom` triggered the recipient's
/// `tokensReceived` hook BEFORE finalizing balances. The attacker's
/// recipient re-entered the Uniswap V1 exchange's swap function,
/// getting favorable pricing twice from the same reserve state.
/// Catalog template encodes the structural anti-pattern (hook before
/// state-finalization, no shared reentrancy lock).
contract ImBTCPool {
    mapping(address => uint256) public balanceOf;
    uint256 public transferCount;

    function deposit(uint256 amount) external {
        balanceOf[msg.sender] += amount;
    }

    /// Bug: hook fires before balance is finalized. A malicious
    /// recipient re-enters and double-counts.
    function transfer(address to, uint256 amount) external {
        require(balanceOf[msg.sender] >= amount, "ImBTCPool: insufficient");
        ITokenRecipient(to).onTokenReceived(amount);
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        transferCount++;
    }
}
