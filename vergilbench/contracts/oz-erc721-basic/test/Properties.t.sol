// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// After minting tokenId to `to`, ownerOf(tokenId) == to.
    function check_mint_sets_owner(address to, uint256 tokenId) external {
        require(to != address(0));
        nft.mint(to, tokenId);
        assert(nft.ownerOf(tokenId) == to);
    }

    /// Minting an already-minted tokenId reverts.
    function check_double_mint_reverts(address to, uint256 tokenId) external {
        require(to != address(0));
        nft.mint(to, tokenId);
        try nft.mint(to, tokenId) { assert(false); } catch {}
    }
}
