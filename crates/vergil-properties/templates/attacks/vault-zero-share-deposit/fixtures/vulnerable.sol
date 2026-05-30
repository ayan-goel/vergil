// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: shares = assets * totalShares / totalAssets — truncates to
/// zero for small `assets` after the share price is inflated. No
/// positivity guard, so the victim's assets are absorbed.
contract Target {
    uint256 public totalShares;
    uint256 public totalAssets;
    mapping(address => uint256) public sharesOf;

    function deposit(uint256 assets) external returns (uint256 sharesOut) {
        if (totalShares == 0) {
            sharesOut = assets;
        } else {
            sharesOut = (assets * totalShares) / totalAssets;
        }
        // BUG: no `require(sharesOut > 0)`.
        sharesOf[msg.sender] += sharesOut;
        totalShares += sharesOut;
        totalAssets += assets;
    }

    function donate(uint256 assets) external {
        totalAssets += assets;
    }
}
