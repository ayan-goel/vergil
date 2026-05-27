// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITokenLike {
    function balanceOf(address) external view returns (uint256);
    function totalSupply() external view returns (uint256);
    function burn(uint256 amount) external;
}

contract Check_erc20_burn_debits_self_and_total {
    ITokenLike public token;

    function check_burn_debits_self_and_total(uint256 amount) public {
        uint256 balBefore = token.balanceOf(address(this));
        uint256 totalBefore = token.totalSupply();
        try token.burn(amount) {
            assert(token.balanceOf(address(this)) == balBefore - amount);
            assert(token.totalSupply() == totalBefore - amount);
        } catch {
            // Burn reverts if amount > balance; only assert on success.
        }
    }
}
