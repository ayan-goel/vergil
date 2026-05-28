// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IMultisigLike {
    function executed(uint256 txId) external view returns (bool);
    function execute(uint256 txId) external returns (bool);
}

contract Check_access_multisig_execute_once {
    IMultisigLike public lock;

    /// If executed[txId] is true, a fresh execute must revert.
    function check_execute_once_only(uint256 txId) external {
        require(lock.executed(txId));
        try lock.execute(txId) returns (bool) {
            assert(false);
        } catch {}
    }
}
