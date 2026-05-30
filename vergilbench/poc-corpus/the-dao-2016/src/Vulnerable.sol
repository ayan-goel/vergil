// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// The DAO (Jun 2016) — reentrancy in withdrawRewardFor.
///
/// Reproduction note: the original DAO had a ~1200-line WhitepaperDAO
/// implementation; the bug class is "external call to caller with no
/// reentrancy guard, allowing the callback to re-enter the withdraw
/// path before the parent finished." This minimal reproduction
/// preserves the bug shape while exposing realistic deposit / balance
/// surface so the catalog template has to bind through to the
/// vulnerable function, not match on a narrow shape.
contract DAO {
    mapping(address => uint256) public balanceOf;
    uint256 public totalWithdrawals;

    function deposit(uint256 amount) external {
        balanceOf[msg.sender] += amount;
    }

    /// Bug: external call to msg.sender with no reentrancy lock; the
    /// caller's receive() hook can re-enter `withdrawBalance` and
    /// double-bump `totalWithdrawals` before the parent call returns.
    function withdrawBalance() external {
        totalWithdrawals++;
        (bool ok, ) = msg.sender.call("");
        require(ok, "DAO: payout failed");
    }
}
