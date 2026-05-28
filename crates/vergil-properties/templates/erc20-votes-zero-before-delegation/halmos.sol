// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IVotesLike {
    function getVotes(address account) external view returns (uint256);
}

contract Check_erc20_votes_zero_before_delegation {
    IVotesLike internal token;

    function check_votes_zero_before_delegation(address account) external view {
        assert(token.getVotes(account) == 0);
    }
}
