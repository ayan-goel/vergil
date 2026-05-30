// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: `donate` is a no-op — direct transfers are not credited to
/// the rate-defining `totalAssets` slot. Only `deposit` modifies the
/// rate. Mirrors OZ ERC-4626's tracked-totalAssets discipline.
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

    function donate(uint256 /*amount*/) external pure {
        // No-op: tracked accounting is immune to direct asset transfers.
    }
}
