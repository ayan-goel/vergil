// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Token} from "../src/Token.sol";

/// @notice Halmos symbolic properties for the reference ERC-20.
///         `check_*` functions are the entry points Halmos enumerates.
contract Properties {
    Token internal token;

    constructor() {
        token = new Token(1_000_000 ether);
    }

    /// transfer preserves totalSupply (no minting/burning side effect).
    function check_transfer_preserves_total_supply(address to, uint256 amount) external {
        uint256 supplyBefore = token.totalSupply();
        try token.transfer(to, amount) {} catch {}
        assert(token.totalSupply() == supplyBefore);
    }

    /// approve is idempotent: calling twice with the same value leaves allowance unchanged.
    function check_approve_idempotent(address spender, uint256 value) external {
        token.approve(spender, value);
        uint256 a1 = token.allowance(address(this), spender);
        token.approve(spender, value);
        uint256 a2 = token.allowance(address(this), spender);
        assert(a1 == a2);
    }
}
