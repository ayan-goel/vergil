// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// The EIP-191 prefix changes the digest (it differs from the raw hash) and is deterministic.
    function check_prefix_changes_and_is_stable(bytes32 h) external view {
        bytes32 d = c.ethSigned(h);
        assert(d != h);
        assert(d == c.ethSigned(h));
    }
}
