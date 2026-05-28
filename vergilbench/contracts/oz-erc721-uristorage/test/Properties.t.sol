// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// After minting with a URI, ownerOf is set (URI storage does not break ownership).
    function check_mint_with_uri_sets_owner(address to, uint256 tokenId) external {
        require(to != address(0));
        nft.mint(to, tokenId, "ipfs://x");
        assert(nft.ownerOf(tokenId) == to);
    }

    /// tokenURI of a nonexistent token reverts.
    function check_uri_of_missing_token_reverts(uint256 tokenId) external {
        try nft.tokenURI(tokenId) { assert(false); } catch {}
    }
}
