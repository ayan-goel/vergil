// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IEcdsaTryLike {
    function tryRecoverLen(bytes32 hash, bytes calldata sig) external pure returns (address, uint8);
}

contract Check_crypto_ecdsa_bad_length_error {
    IEcdsaTryLike internal helper;

    function check_bad_length_is_error(bytes32 hash) external view {
        (address rec, uint8 err) = helper.tryRecoverLen(hash, hex"1234");
        assert(rec == address(0));
        assert(err != 0);
    }
}
