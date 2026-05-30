// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: explicit positivity guard refuses dust-rounding deposits.
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
        require(sharesOut > 0, "Target: zero shares");
        sharesOf[msg.sender] += sharesOut;
        totalShares += sharesOut;
        totalAssets += assets;
    }

    function donate(uint256 assets) external {
        totalAssets += assets;
    }
}
