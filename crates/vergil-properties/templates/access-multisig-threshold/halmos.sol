// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IMultisigLike {
    function confirmationCount(uint256 txId) external view returns (uint256);
    function execute(uint256 txId) external returns (bool);
}

contract Check_access_multisig_threshold {
    IMultisigLike public lock;
    // Threshold the multisig enforces; set in scaffold constructor.
    uint256 public threshold;

    /// execute reverts when confirmationCount[txId] < threshold.
    function check_execute_requires_threshold(uint256 txId) external {
        require(lock.confirmationCount(txId) < threshold);
        try lock.execute(txId) returns (bool) {
            assert(false);
        } catch {}
    }
}
