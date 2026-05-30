// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: `abi.encode` length-prefixes each field, so distinct
/// (a, b) pairs always produce distinct encodings.
contract Target {
    function identify(bytes memory a, bytes memory b) external pure returns (bytes32) {
        return keccak256(abi.encode(a, b));
    }
}
