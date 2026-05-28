// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal multi;
    constructor() { multi = new Contract(); }

    /// totalSupply(id) grows by exactly the minted amount.
    function check_mint_tracks_supply(address to, uint256 id, uint256 amount) external {
        require(to != address(0));
        uint256 before = multi.totalSupply(id);
        multi.mint(to, id, amount);
        assert(multi.totalSupply(id) == before + amount);
    }

    /// exists(id) is false for an unminted id.
    function check_unminted_does_not_exist(uint256 id) external view {
        require(multi.totalSupply(id) == 0);
        assert(!multi.exists(id));
    }
}
