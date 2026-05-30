// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: minimal ERC-4626-like vault. The first depositor controls
/// the initial share-asset ratio, and a subsequent direct asset transfer
/// (`donate`) inflates the ratio further. A small deposit then rounds to
/// zero shares while consuming the asset.
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
        // BUG: no check that sharesOut > 0 — caller's assets are absorbed
        // even when they get zero shares.
        sharesOf[msg.sender] += sharesOut;
        totalShares += sharesOut;
        totalAssets += assets;
    }

    /// Models a direct asset transfer to the vault — the inflation lever.
    function donate(uint256 assets) external {
        totalAssets += assets;
    }
}
