// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 5 S4 fixture — conservation negative case.
//
// `mint` only credits — no matching debit. Total supply changes, so the
// miner must NOT emit a sum-preservation candidate.
contract ConservationMint {
    mapping(address => uint256) public balanceOf;

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
    }
}
