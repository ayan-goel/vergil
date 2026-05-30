// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public authorizedSigner; // uninitialized = address(0)
    bool public authorized;

    function authorize(bytes32 h, uint8 v, bytes32 r, bytes32 s) external {
        address signer = ecrecover(h, v, r, s);
        // BUG: no check that signer != address(0). ecrecover returns 0
        // for invalid signatures; combined with an uninitialized
        // authorizedSigner (also 0), any invalid signature passes.
        require(signer == authorizedSigner, "Target: bad signer");
        authorized = true;
    }
}
