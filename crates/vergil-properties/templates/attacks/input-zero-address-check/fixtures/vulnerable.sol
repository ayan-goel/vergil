// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Target {
    mapping(address => uint256) public registry;
    uint256 public nextId = 1;
    // BUG: no zero-address check; registers address(0).
    function register(address a) external returns (uint256 id) {
        id = nextId++;
        registry[a] = id;
    }
}
