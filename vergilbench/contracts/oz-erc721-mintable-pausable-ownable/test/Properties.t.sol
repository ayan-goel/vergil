// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// Owner-gated auto-increment mint assigns consecutive ids.
    function check_autoincrement_ids(address a, address b) external {
        require(a != address(0) && b != address(0));
        uint256 i1 = nft.safeMint(a);
        uint256 i2 = nft.safeMint(b);
        assert(i2 == i1 + 1);
    }

    /// Paused mint reverts.
    function check_paused_blocks_mint(address to) external {
        require(to != address(0));
        nft.pause();
        try nft.safeMint(to) { assert(false); } catch {}
    }
}
