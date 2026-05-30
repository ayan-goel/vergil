// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Cream Finance AMP cross-function reentrancy (Aug 2021).
///
/// Reproduction note: Cream's AMP borrow flow called into the AMP
/// token (ERC-777-style), whose `_callPostTransferHooks` invoked the
/// recipient's hook BEFORE the borrow's accounting was finalized.
/// The hook re-entered a different Cream function (also `borrow`
/// against a separate asset), double-counting the collateral.
///
/// The catalog template (`reentrancy-cross-function-state`) abstracts
/// the bug as two functions `tick()` and `step()` sharing a counter
/// invariant with no shared lock. We preserve those names here for
/// the binding-free common surface; the historical mapping is
/// tick=`borrow with hook`, step=`borrow without hook` (or any
/// state-changing siblings sharing the invariant).
contract CreamLendingPool {
    uint256 public counter;
    mapping(address => uint256) public borrowedBalanceOf;

    /// External-callback-issuing function. Hook called via msg.sender.call("")
    /// stands in for the AMP token's post-transfer notification.
    function tick() external {
        counter++;
        borrowedBalanceOf[msg.sender]++;
        (bool ok, ) = msg.sender.call("");
        require(ok, "Cream: callback failed");
    }

    /// Cross-function re-entry target. No shared reentrancy lock with
    /// `tick`, so the hook from `tick` can call this and double-bump
    /// the shared `counter`.
    function step() external {
        counter++;
        borrowedBalanceOf[msg.sender]++;
    }
}
