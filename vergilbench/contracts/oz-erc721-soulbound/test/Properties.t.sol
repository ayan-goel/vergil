// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// Minting (from the zero address) is allowed.
    function check_mint_allowed(address to, uint256 tokenId) external {
        require(to != address(0));
        nft.mint(to, tokenId);
        assert(nft.ownerOf(tokenId) == to);
    }

    /// Transferring an owned token reverts (soulbound).
    function check_transfer_blocked(uint256 tokenId) external {
        nft.mint(address(this), tokenId);
        try nft.transferFrom(address(this), address(0xBEEF), tokenId) { assert(false); } catch {}
    }
}
