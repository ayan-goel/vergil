// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// Burning decrements the enumerable total supply by one.
    function check_burn_decrements_supply(uint256 tokenId) external {
        nft.mint(address(this), tokenId);
        uint256 before = nft.totalSupply();
        nft.burn(tokenId);
        assert(nft.totalSupply() == before - 1);
    }
}
