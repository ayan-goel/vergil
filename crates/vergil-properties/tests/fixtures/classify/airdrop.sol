// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Airdrop {
    bytes32 public merkleRoot;
    mapping(address => bool) public claimed;
    function claim(uint256 amount, bytes32[] calldata proof) external { /* ... */ }
}
