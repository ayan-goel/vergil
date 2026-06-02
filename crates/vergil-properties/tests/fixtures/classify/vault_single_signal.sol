// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 3 S2 fixture — vault-ish contract with ONLY one signal
// (asset() getter alone). Confidence falls to 0.70 — surfaced but
// below the activation threshold 0.6 + buffer.
contract MaybeVault {
    address public underlying;

    function asset() external view returns (address) {
        return underlying;
    }
}
