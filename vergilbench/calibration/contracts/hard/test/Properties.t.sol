// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Vault} from "../src/Vault.sol";

contract Properties {
    Vault internal target;
    constructor() { target = new Vault(); }

    function check_balance_le_total(uint128 amount) external {
        target.deposit(amount);
        assert(target.balanceOf(address(this)) <= target.totalDeposited());
    }
}
