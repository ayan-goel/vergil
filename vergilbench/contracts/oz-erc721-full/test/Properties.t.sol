// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// Owner-gated mint increases the enumerable total supply by one.
    function check_mint_increments_supply(address to, uint256 tokenId) external {
        require(to != address(0));
        uint256 before = nft.totalSupply();
        nft.safeMint(to, tokenId, "ipfs://x");
        assert(nft.totalSupply() == before + 1);
    }

    /// While paused, minting reverts (the merged _update chain enforces it).
    function check_paused_blocks_mint(address to, uint256 tokenId) external {
        require(to != address(0));
        nft.pause();
        try nft.safeMint(to, tokenId, "ipfs://x") { assert(false); } catch {}
    }
}
