// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: trivial monotone implementation. Real vaults use
/// `assets * totalShares / totalAssets` with consistent rounding; the
/// 1:1 ratio here is the simplest function that preserves the property.
contract Target {
    function convertToShares(uint256 assets) external pure returns (uint256) {
        return assets;
    }
}
