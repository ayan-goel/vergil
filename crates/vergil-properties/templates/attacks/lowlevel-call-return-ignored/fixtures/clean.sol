// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: explicit `require(ok)` ensures a failed call doesn't credit
/// the per-recipient counter.
contract Target {
    mapping(address => uint256) public settledCount;

    function settle(address recipient, bytes calldata data) external {
        (bool ok, ) = recipient.call(data);
        require(ok, "Target: settle failed");
        settledCount[recipient]++;
    }
}
