// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// ============================================================================
// VENDORED VERBATIM from DeFiVulnLabs
//   Source:  https://github.com/SunWeb3Sec/DeFiVulnLabs
//   File:    src/test/Hash-collisions.sol  (Aug 2023)
//   Commit:  main branch as of fetch
//   License: MIT (Apache-compatible)
// ============================================================================
// HashCollisionBug is the exact contract under test from DVL's reduction.
// Only `deposit()` is omitted from the original — it uses `msg.value`, which
// the Halmos bare scaffold doesn't model. The vulnerability (`createHash`
// via `abi.encodePacked` over two dynamic-length strings) is preserved
// verbatim. The bug class is SWC-133.
contract HashCollisionBug {
    mapping(bytes32 => uint256) public balances;

    function createHash(
        string memory _string1,
        string memory _string2
    ) public pure returns (bytes32) {
        return keccak256(abi.encodePacked(_string1, _string2));
    }
}

// ============================================================================
// VERGIL ADAPTER (not vendored)
// ============================================================================
// Vergil's `quirk-abi-encode-packed-collision` template hard-codes the
// surface `target.identify(bytes,bytes) returns (bytes32)`. DeFiVulnLabs's
// `HashCollisionBug.createHash` has signature `(string,string)`. This
// adapter exposes the template's required binding by routing to the
// vendored `createHash` via `bytes → string` casting (Solidity's UTF-8
// passthrough — no semantic transformation).
//
// What this means for independence: the bug behavior (abi.encodePacked
// collision) lives in the vendored DVL contract. The adapter is a thin
// type-shape mediator; it can't introduce or hide a bug. If Halmos finds
// a cex through the adapter, the cex traces back to DVL's verbatim
// `createHash`.
//
// **V2 plan documented in `vergilbench/poc-corpus/README.md` §gap-2**:
// remove the adapter by making the template's function name and argument
// types bindable. This unblocks vendoring DVL files directly without the
// adapter layer.
contract Target {
    HashCollisionBug public bug;

    constructor() {
        bug = new HashCollisionBug();
    }

    function identify(bytes memory a, bytes memory b) external view returns (bytes32) {
        return bug.createHash(string(a), string(b));
    }
}
