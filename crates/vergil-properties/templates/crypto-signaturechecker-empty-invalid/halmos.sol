// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ISigCheckerLike {
    function valid(address signer, bytes32 hash, bytes calldata sig) external view returns (bool);
}

contract Check_crypto_signaturechecker_empty_invalid {
    ISigCheckerLike internal helper;

    function check_empty_sig_invalid(address signer, bytes32 hash) external view {
        require(signer != address(0));
        assert(!helper.valid(signer, hash, ""));
    }
}
