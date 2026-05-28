// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal multi;
    constructor() { multi = new Contract(); }

    /// While paused, minting reverts.
    function check_paused_blocks_mint(address to, uint256 id, uint256 amount) external {
        require(to != address(0));
        multi.pause();
        try multi.mint(to, id, amount) { assert(false); } catch {}
    }
}
