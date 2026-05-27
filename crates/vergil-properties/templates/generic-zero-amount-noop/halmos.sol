// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITokenLike {
    function transfer(address, uint256) external returns (bool);
    function balanceOf(address) external view returns (uint256);
}

contract Check_generic_zero_amount_noop {
    ITokenLike public token;

    function check_transfer_zero_does_not_change_balances(address to) public {
        uint256 a0 = token.balanceOf(address(this));
        uint256 b0 = token.balanceOf(to);
        try token.transfer(to, 0) {
            assert(token.balanceOf(address(this)) == a0);
            assert(token.balanceOf(to) == b0);
        } catch {}
    }
}
