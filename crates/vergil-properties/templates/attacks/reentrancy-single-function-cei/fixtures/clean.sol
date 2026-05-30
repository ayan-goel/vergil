// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean contract: `action` is protected by a hand-rolled `nonReentrant`
/// modifier that sets a `locked` flag before the body and clears it after.
/// A re-entrant call hits the `require(!locked)` and reverts; the outer
/// call observes the revert via try/catch and the counter is only
/// incremented once.
contract Target {
    uint256 public counter;
    bool private locked;

    modifier nonReentrant() {
        require(!locked, "Target: reentrant");
        locked = true;
        _;
        locked = false;
    }

    function action() external nonReentrant {
        counter++;
        (bool ok, ) = msg.sender.call("");
        require(ok, "Target: external call failed");
    }
}
