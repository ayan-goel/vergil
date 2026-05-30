// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Target {
    mapping(address => uint256) public registry;
    uint256 public nextId = 1;
    function register(address a) external returns (uint256 id) {
        require(a != address(0), "Target: zero address");
        id = nextId++;
        registry[a] = id;
    }
}
