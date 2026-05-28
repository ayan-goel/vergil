// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal multi;
    constructor() { multi = new Contract(); }

    /// Burning debits the holder's balance by exactly the burned amount.
    function check_burn_debits_balance(uint256 id, uint256 amount) external {
        multi.mint(address(this), id, amount);
        uint256 before = multi.balanceOf(address(this), id);
        multi.burn(address(this), id, amount);
        assert(multi.balanceOf(address(this), id) == before - amount);
    }
}
