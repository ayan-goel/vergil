// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// After the owner burns a token, querying its owner reverts.
    function check_burn_clears_owner(uint256 tokenId) external {
        nft.mint(address(this), tokenId);
        nft.burn(tokenId);
        try nft.ownerOf(tokenId) { assert(false); } catch {}
    }

    /// A non-owner/non-approved caller cannot burn.
    function check_unauthorized_burn_reverts(address other, uint256 tokenId) external {
        require(other != address(this) && other != address(0));
        nft.mint(other, tokenId);
        try nft.burn(tokenId) { assert(false); } catch {}
    }
}
