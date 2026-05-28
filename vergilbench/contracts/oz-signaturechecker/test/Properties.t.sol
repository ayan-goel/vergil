// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// An empty signature against a non-zero EOA signer is not valid.
    function check_empty_sig_invalid(address signer, bytes32 hash) external view {
        require(signer != address(0));
        assert(!c.valid(signer, hash, ""));
    }
}
