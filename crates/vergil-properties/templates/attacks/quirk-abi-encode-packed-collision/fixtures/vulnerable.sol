// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: identify-by-hash uses `abi.encodePacked` over two
/// dynamic-length values. Adjacent bytes can be shifted across the
/// field boundary, producing the same hash for distinct (a, b) pairs.
contract Target {
    function identify(bytes memory a, bytes memory b) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, b));
    }
}
