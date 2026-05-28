// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// A fresh account's nonce starts at zero.
    function check_initial_nonce_zero(address owner) external view {
        assert(c.nonces(owner) == 0);
    }

    /// Consuming a nonce increments it by exactly one.
    function check_use_increments_nonce(address owner) external {
        uint256 before = c.nonces(owner);
        c.use(owner);
        assert(c.nonces(owner) == before + 1);
    }
}
