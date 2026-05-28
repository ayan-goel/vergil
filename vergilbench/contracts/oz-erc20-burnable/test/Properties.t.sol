// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(1_000_000e18); }

    /// burn debits the caller's balance and total supply by exactly `amount`.
    function check_burn_reduces_supply(uint256 amount) external {
        require(amount <= token.balanceOf(address(this)));
        uint256 s = token.totalSupply();
        uint256 b = token.balanceOf(address(this));
        token.burn(amount);
        assert(token.totalSupply() == s - amount);
        assert(token.balanceOf(address(this)) == b - amount);
    }

    /// burning more than the balance reverts.
    function check_burn_over_balance_reverts(uint256 amount) external {
        require(amount > token.balanceOf(address(this)));
        try token.burn(amount) { assert(false); } catch {}
    }
}
