// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Chainlink-shape price feed.
contract PriceFeed {
    int256 public latestAnswer;
    function decimals() external view returns (uint8) { return 8; }
}
