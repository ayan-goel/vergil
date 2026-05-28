// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// While paused, minting reverts.
    function check_paused_blocks_mint(address to, uint256 tokenId) external {
        require(to != address(0));
        nft.pause();
        try nft.mint(to, tokenId) { assert(false); } catch {}
    }

    /// unpause clears the paused flag.
    function check_unpause_clears_flag() external {
        nft.pause();
        nft.unpause();
        assert(!nft.paused());
    }
}
