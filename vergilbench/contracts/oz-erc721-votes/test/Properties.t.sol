// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// Before any delegation, an account has zero voting weight.
    function check_initial_votes_zero(address account) external view {
        assert(nft.getVotes(account) == 0);
    }

    /// After self-delegation, voting weight equals the NFT balance.
    function check_self_delegate_matches_balance() external {
        nft.mint(address(this), 1);
        nft.delegate(address(this));
        assert(nft.getVotes(address(this)) == nft.balanceOf(address(this)));
    }
}
