// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// Mint increments enumerable supply by one.
    function check_mint_increments_supply(address to, uint256 tokenId) external {
        require(to != address(0));
        uint256 before = nft.totalSupply();
        nft.mint(to, tokenId);
        assert(nft.totalSupply() == before + 1);
    }

    /// Paused mint reverts (merged update chain).
    function check_paused_blocks_mint(address to, uint256 tokenId) external {
        require(to != address(0));
        nft.pause();
        try nft.mint(to, tokenId) { assert(false); } catch {}
    }
}
