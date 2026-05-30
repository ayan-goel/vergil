// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable contract: `action` increments the counter and then makes an
/// external call to msg.sender. With no reentrancy guard, the caller's
/// receive() hook can re-enter `action` and double-increment. The DAO
/// (Jun 2016, ~3.6M ETH / ~$60M at the time) is the canonical instance.
contract Target {
    uint256 public counter;

    function action() external {
        counter++;
        // BUG: external call after state change but no reentry guard.
        (bool ok, ) = msg.sender.call("");
        require(ok, "Target: external call failed");
    }
}
