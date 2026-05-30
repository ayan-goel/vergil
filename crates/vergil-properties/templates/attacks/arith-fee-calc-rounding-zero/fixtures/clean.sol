// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Target {
    uint256 public constant RATE = 5;
    function feeFor(uint256 amount) external pure returns (uint256) {
        uint256 fee = (amount * RATE) / 10000;
        return fee == 0 ? 1 : fee; // floor at 1 wei
    }
}
