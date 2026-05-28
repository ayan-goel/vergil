// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {SignatureChecker} from "@openzeppelin/contracts/utils/cryptography/SignatureChecker.sol";

contract Contract {
    function valid(address signer, bytes32 hash, bytes calldata sig) external view returns (bool) {
        return SignatureChecker.isValidSignatureNow(signer, hash, sig);
    }
}
