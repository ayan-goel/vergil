// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Target {
    address public authorizedSigner;
    bool public authorized;

    function authorize(bytes32 h, uint8 v, bytes32 r, bytes32 s) external {
        address signer = ecrecover(h, v, r, s);
        // Defensive: reject zero-address recovered signers.
        require(signer != address(0), "Target: invalid signature");
        require(signer == authorizedSigner, "Target: bad signer");
        authorized = true;
    }
}
