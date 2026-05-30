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

/// Clean: protocol gates the oracle price on staleness.
contract Target {
    Oracle public immutable oracle;
    uint256 public constant MAX_STALENESS = 3600; // 1 hour

    constructor() {
        oracle = new Oracle();
    }

    function usePrice() external view returns (uint256) {
        (uint256 price, uint256 updatedAt) = oracle.latestRoundData();
        require(updatedAt > 0, "Target: missing timestamp");
        require(block.timestamp - updatedAt <= MAX_STALENESS, "Target: stale price");
        return price;
    }
}
