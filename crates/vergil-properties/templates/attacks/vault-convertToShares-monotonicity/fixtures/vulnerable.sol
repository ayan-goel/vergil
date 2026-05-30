// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: a contrived discount-band breaks monotonicity. Stands in
/// for the rounding-direction bugs seen in real ERC-4626 implementations
/// where preview rounds one way and execution rounds the other.
contract Target {
    function convertToShares(uint256 assets) external pure returns (uint256) {
        // BUG: for assets > 1000, subtract a 500-unit "bonus" — this
        // inverts the curve at the threshold and breaks monotonicity.
        if (assets > 1000) {
            return assets - 500;
        }
        return assets;
    }
}
