// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// totalSupply increases by one per mint.
    function check_mint_increments_total_supply(address to, uint256 tokenId) external {
        require(to != address(0));
        uint256 before = nft.totalSupply();
        nft.mint(to, tokenId);
        assert(nft.totalSupply() == before + 1);
    }

    /// A freshly deployed collection has zero supply.
    function check_initial_supply_zero() external view {
        assert(nft.totalSupply() == 0);
    }
}
