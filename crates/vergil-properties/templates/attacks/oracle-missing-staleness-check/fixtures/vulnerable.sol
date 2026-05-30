// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Oracle {
    uint256 public price;
    uint256 public updatedAt;
    function setPrice(uint256 p, uint256 u) external {
        price = p;
        updatedAt = u;
    }
    function latestRoundData() external view returns (uint256, uint256) {
        return (price, updatedAt);
    }
}

/// Vulnerable: protocol reads the oracle's price but doesn't check the
/// `updatedAt` timestamp. Stale prices are accepted.
contract Target {
    Oracle public immutable oracle;
    uint256 public constant MAX_STALENESS = 3600; // 1 hour

    constructor() {
        oracle = new Oracle();
    }

    function usePrice() external view returns (uint256) {
        (uint256 price, ) = oracle.latestRoundData();
        // BUG: updatedAt is read but never gated.
        return price;
    }
}
