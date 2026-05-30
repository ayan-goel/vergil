// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: `donate` credits `totalAssets` without minting shares,
/// shifting the conversion rate. Stands in for the direct-transfer
/// channel (selfdestruct, ERC-20 transfer, rebase) that the vulnerable
/// exchange-rate computation reads.
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
        sharesOf[msg.sender] += sharesOut;
        totalShares += sharesOut;
        totalAssets += assets;
    }

    function convertToShares(uint256 assets) external view returns (uint256) {
        if (totalShares == 0) return assets;
        return (assets * totalShares) / totalAssets;
    }

    function donate(uint256 amount) external {
        // BUG: direct asset transfer credited to the rate-defining storage slot.
        totalAssets += amount;
    }
}
