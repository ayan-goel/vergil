// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(bytes32(uint256(0x1234))); }

    /// A claim with an empty proof against a nonzero root reverts.
    function check_empty_proof_reverts(address account) external {
        bytes32[] memory proof = new bytes32[](0);
        try c.claim(account, proof) { assert(false); } catch {}
    }

    /// No account is marked claimed before any successful claim.
    function check_initially_unclaimed(address account) external view {
        assert(!c.claimed(account));
    }
}
