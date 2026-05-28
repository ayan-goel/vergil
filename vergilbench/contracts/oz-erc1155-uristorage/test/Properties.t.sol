// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal multi;
    constructor() { multi = new Contract(); }

    function check_mint_credits_balance(address to, uint256 id, uint256 amount) external {
        require(to != address(0));
        uint256 before = multi.balanceOf(to, id);
        multi.mint(to, id, amount);
        assert(multi.balanceOf(to, id) == before + amount);
    }
}
