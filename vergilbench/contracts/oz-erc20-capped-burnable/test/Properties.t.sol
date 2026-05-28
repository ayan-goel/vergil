// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    /// The cap invariant holds even though tokens are burnable.
    function check_supply_at_most_cap() external view {
        assert(token.totalSupply() <= token.cap());
    }

    /// Burning then the cap invariant still holds.
    function check_burn_keeps_cap_invariant(uint256 amount) external {
        require(amount <= token.balanceOf(address(this)));
        token.burn(amount);
        assert(token.totalSupply() <= token.cap());
    }
}
