// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(1_000_000e18, 500_000e18); }

    /// The cap invariant: total supply is always at or below the cap.
    function check_supply_at_most_cap() external view {
        assert(token.totalSupply() <= token.cap());
    }

    /// A mint that would breach the cap reverts.
    function check_mint_over_cap_reverts(address to, uint256 amount) external {
        require(to != address(0));
        require(amount > token.cap() - token.totalSupply());
        try token.mint(to, amount) { assert(false); } catch {}
    }
}
