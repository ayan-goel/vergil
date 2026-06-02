// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 3 S1 fixture — ERC-1155 minimal surface.
//
// Has the distinguishing `safeBatchTransferFrom` + `balanceOfBatch`
// signatures that separate ERC-1155 from ERC-20 / ERC-721.
contract Erc1155Minimal {
    mapping(uint256 => mapping(address => uint256)) public balances;

    function balanceOf(address owner, uint256 id) external view returns (uint256) {
        return balances[id][owner];
    }

    function balanceOfBatch(
        address[] calldata owners,
        uint256[] calldata ids
    ) external view returns (uint256[] memory) {
        uint256[] memory out = new uint256[](owners.length);
        for (uint256 i = 0; i < owners.length; i++) {
            out[i] = balances[ids[i]][owners[i]];
        }
        return out;
    }

    function safeTransferFrom(address from, address to, uint256 id, uint256 amount, bytes calldata) external {
        balances[id][from] -= amount;
        balances[id][to] += amount;
    }

    function safeBatchTransferFrom(
        address from,
        address to,
        uint256[] calldata ids,
        uint256[] calldata amounts,
        bytes calldata
    ) external {
        for (uint256 i = 0; i < ids.length; i++) {
            balances[ids[i]][from] -= amounts[i];
            balances[ids[i]][to] += amounts[i];
        }
    }
}
