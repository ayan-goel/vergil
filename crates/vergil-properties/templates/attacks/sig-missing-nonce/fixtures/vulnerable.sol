// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: `executeWithProof` validates the hash but doesn't mark it
/// as consumed — the same proof replays indefinitely.
contract Target {
    uint256 public counter;

    function executeWithProof(bytes32 proof, uint256 amount) external {
        require(
            keccak256(abi.encodePacked(amount, msg.sender)) == proof,
            "Target: bad proof"
        );
        // BUG: proof not recorded as consumed.
        counter += amount;
    }
}
