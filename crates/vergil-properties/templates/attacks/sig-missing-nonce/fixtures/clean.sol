// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: consumed proofs are recorded; replay reverts.
contract Target {
    uint256 public counter;
    mapping(bytes32 => bool) public consumed;

    function executeWithProof(bytes32 proof, uint256 amount) external {
        require(
            keccak256(abi.encodePacked(amount, msg.sender)) == proof,
            "Target: bad proof"
        );
        require(!consumed[proof], "Target: replay");
        consumed[proof] = true;
        counter += amount;
    }
}
