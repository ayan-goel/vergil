// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IMerkleClaimLike {
    function claim(address account, bytes32[] calldata proof) external;
}

contract Check_crypto_merkleproof_empty_proof_fails {
    IMerkleClaimLike internal airdrop;

    function check_empty_proof_reverts(address account) external {
        bytes32[] memory proof = new bytes32[](0);
        try airdrop.claim(account, proof) { assert(false); } catch {}
    }
}
