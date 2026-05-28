// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    /// Voting power is zero until an account delegates.
    function check_votes_zero_before_delegation(address account) external view {
        assert(token.getVotes(account) == 0);
    }

    /// After self-delegation, voting power equals the token balance.
    function check_self_delegate_matches_balance(uint256 amount) external {
        require(amount <= 1e30);
        token.mint(address(this), amount);
        token.delegate(address(this));
        assert(token.getVotes(address(this)) == token.balanceOf(address(this)));
    }
}
