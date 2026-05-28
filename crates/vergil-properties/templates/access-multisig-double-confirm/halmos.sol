// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IMultisigLike {
    function confirmed(uint256 txId, address signer) external view returns (bool);
    function confirm(uint256 txId) external;
}

contract Check_access_multisig_double_confirm {
    IMultisigLike public lock;

    /// If the signer (msg.sender) already confirmed, a second confirm
    /// must revert.
    function check_double_confirm_reverts(uint256 txId) external {
        require(lock.confirmed(txId, address(this)));
        try lock.confirm(txId) {
            assert(false);
        } catch {}
    }
}
