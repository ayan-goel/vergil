// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    /// maxFlashLoan equals the headroom up to type(uint256).max minus supply.
    function check_max_flashloan_is_headroom() external view {
        assert(token.maxFlashLoan(address(token)) == type(uint256).max - token.totalSupply());
    }

    /// burn reduces supply by exactly the burned amount.
    function check_burn_reduces_supply(uint256 amount) external {
        require(amount <= token.balanceOf(address(this)));
        uint256 before = token.totalSupply();
        token.burn(amount);
        assert(token.totalSupply() == before - amount);
    }
}
