// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";

contract Contract {
    function tryRecoverLen(bytes32 hash, bytes calldata sig) external pure returns (address, uint8) {
        (address rec, ECDSA.RecoverError err,) = ECDSA.tryRecover(hash, sig);
        return (rec, uint8(err));
    }
}
