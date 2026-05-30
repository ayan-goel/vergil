// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: push-payment style — `distribute` calls both recipients
/// and requires both succeed. A single refusing recipient blocks the
/// whole distribution.
contract Target {
    mapping(address => uint256) public credited;

    function distribute(address a, address b, uint256 amount) external {
        // BUG: require(ok) lets one failing call halt the whole distribution.
        (bool ok1, ) = a.call("");
        require(ok1, "Target: a refused");
        credited[a] += amount;
        (bool ok2, ) = b.call("");
        require(ok2, "Target: b refused");
        credited[b] += amount;
    }
}
