// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: two functions share a counter invariant. `tick` calls back
/// to the caller; `step` updates the counter directly. Neither has a
/// reentrancy guard, so a malicious caller re-enters `step` from `tick`'s
/// callback and double-increments. Cream Finance (Aug 2021) is the
/// canonical instance, scaled to AMP-borrow / re-borrow accounting.
contract Target {
    uint256 public counter;

    function tick() external {
        counter++;
        (bool ok, ) = msg.sender.call("");
        require(ok, "Target: callback failed");
    }

    function step() external {
        counter++;
    }
}
