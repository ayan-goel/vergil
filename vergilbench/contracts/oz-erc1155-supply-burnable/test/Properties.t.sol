// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal multi;
    constructor() { multi = new Contract(); }

    /// Burning reduces per-id total supply by exactly the burned amount.
    function check_burn_reduces_supply(uint256 id, uint256 amount) external {
        multi.mint(address(this), id, amount);
        uint256 s = multi.totalSupply(id);
        multi.burn(address(this), id, amount);
        assert(multi.totalSupply(id) == s - amount);
    }
}
