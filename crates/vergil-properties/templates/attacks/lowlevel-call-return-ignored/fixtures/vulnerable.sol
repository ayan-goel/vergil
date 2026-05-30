// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: `settle` ignores the call's success flag and credits
/// `settledCount` regardless. A failing recipient still gets credited.
contract Target {
    mapping(address => uint256) public settledCount;

    function settle(address recipient, bytes calldata data) external {
        // BUG: return ignored.
        recipient.call(data);
        settledCount[recipient]++;
    }
}
