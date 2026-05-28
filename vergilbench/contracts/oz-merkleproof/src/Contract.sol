// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {MerkleProof} from "@openzeppelin/contracts/utils/cryptography/MerkleProof.sol";

/// A Merkle-allowlist claim contract (real-world airdrop pattern).
contract Contract {
    bytes32 public immutable root;
    mapping(address => bool) public claimed;

    constructor(bytes32 root_) { root = root_; }

    function claim(address account, bytes32[] calldata proof) external {
        require(!claimed[account], "claimed");
        bytes32 leaf = keccak256(abi.encodePacked(account));
        require(MerkleProof.verify(proof, root, leaf), "bad proof");
        claimed[account] = true;
    }
}
