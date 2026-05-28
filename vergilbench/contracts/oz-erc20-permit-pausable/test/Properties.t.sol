// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    /// Permit nonce starts at zero.
    function check_initial_nonce_zero(address owner) external view {
        assert(token.nonces(owner) == 0);
    }

    /// Paused transfers revert.
    function check_paused_blocks_transfer(address to, uint256 amount) external {
        token.pause();
        try token.transfer(to, amount) { assert(false); } catch {}
    }
}
