// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    /// Owner mint increases total supply by exactly the minted amount.
    function check_mint_increases_supply(address to, uint256 amount) external {
        require(to != address(0));
        uint256 before = token.totalSupply();
        token.mint(to, amount);
        assert(token.totalSupply() == before + amount);
    }

    /// burn reduces total supply by exactly the burned amount.
    function check_burn_reduces_supply(uint256 amount) external {
        token.mint(address(this), amount);
        uint256 before = token.totalSupply();
        token.burn(amount);
        assert(token.totalSupply() == before - amount);
    }
}
