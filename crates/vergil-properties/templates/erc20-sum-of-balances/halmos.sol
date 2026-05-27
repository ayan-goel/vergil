// SPDX-License-Identifier: Apache-2.0
// Vergil property template: erc20-sum-of-balances
//
// Property: for any pair of accounts (a, b) and any transferFrom action,
// the sum (balanceOf[a] + balanceOf[b]) before equals after — i.e. the
// transfer moves value rather than minting or burning. This is the
// pairwise-conservation lemma that, applied across all touched accounts,
// implies sum(balances) == totalSupply.
//
// Full sum-of-balances would require a ghost-sum tracker, which Halmos
// can model via additional storage. We model the pairwise check here;
// the ghost-sum invariant lives in smtchecker.sol for CHC discharge.

pragma solidity ^0.8.0;

interface ITokenLike {
    function balanceOf(address) external view returns (uint256);
    function totalSupply() external view returns (uint256);
    function transferFrom(address, address, uint256) external returns (bool);
}

contract Check_erc20_sum_of_balances {
    ITokenLike public token;

    function check_transferFrom_preserves_pair_sum(
        address from,
        address to,
        uint256 amount
    ) public {
        require(from != to, "different accounts");
        uint256 a0 = token.balanceOf(from);
        uint256 b0 = token.balanceOf(to);
        try token.transferFrom(from, to, amount) {} catch { return; }
        uint256 a1 = token.balanceOf(from);
        uint256 b1 = token.balanceOf(to);
        assert(a0 + b0 == a1 + b1);
    }
}
