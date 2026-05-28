// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    /// Permit nonce starts at zero (shared Nonces base resolved correctly).
    function check_initial_nonce_zero(address owner) external view {
        assert(token.nonces(owner) == 0);
    }

    /// After self-delegation, votes equal the balance.
    function check_self_delegate_matches_balance(uint256 amount) external {
        require(amount <= 1e30);
        token.mint(address(this), amount);
        token.delegate(address(this));
        assert(token.getVotes(address(this)) == token.balanceOf(address(this)));
    }
}
