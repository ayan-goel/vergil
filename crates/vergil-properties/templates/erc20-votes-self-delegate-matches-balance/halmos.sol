// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IVotesBalanceLike {
    function balanceOf(address account) external view returns (uint256);
    function getVotes(address account) external view returns (uint256);
    function delegate(address delegatee) external;
}

contract Check_erc20_votes_self_delegate_matches_balance {
    IVotesBalanceLike internal token;

    function check_self_delegate_matches_balance() external {
        token.delegate(address(this));
        assert(token.getVotes(address(this)) == token.balanceOf(address(this)));
    }
}
