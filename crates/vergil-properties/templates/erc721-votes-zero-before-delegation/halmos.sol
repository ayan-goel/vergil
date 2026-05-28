// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC721VotesLike {
    function getVotes(address account) external view returns (uint256);
}

contract Check_erc721_votes_zero_before_delegation {
    IERC721VotesLike internal token;

    function check_votes_zero_before_delegation(address account) external view {
        assert(token.getVotes(account) == 0);
    }
}
