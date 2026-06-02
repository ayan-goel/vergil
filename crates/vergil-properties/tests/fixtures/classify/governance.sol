// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Gov {
    function propose(address[] calldata, uint256[] calldata, bytes[] calldata) external returns (uint256) { return 0; }
    function castVote(uint256 proposalId, uint8 support) external returns (uint256) { return 0; }
    function execute(address[] calldata, uint256[] calldata, bytes[] calldata, bytes32) external returns (uint256) { return 0; }
}
