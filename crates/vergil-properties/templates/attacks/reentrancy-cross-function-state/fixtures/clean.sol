// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: a shared reentrancy lock (one slot) guards BOTH functions.
/// `tick`'s callback can re-enter `step`, but the guard reverts it.
contract Target {
    uint256 public counter;
    uint256 private _locked;

    modifier nonReentrant() {
        require(_locked == 0, "Target: reentrant");
        _locked = 1;
        _;
        _locked = 0;
    }

    function tick() external nonReentrant {
        counter++;
        (bool ok, ) = msg.sender.call("");
        require(ok, "Target: callback failed");
    }

    function step() external nonReentrant {
        counter++;
    }
}
