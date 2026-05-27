// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITokenLike {
    function balanceOf(address) external view returns (uint256);
    function transfer(address, uint256) external returns (bool);
}

contract Check_erc20_transfer_conformance {
    ITokenLike public token;

    function check_transfer_credits_recipient_exactly(
        address to,
        uint256 amount
    ) public {
        require(to != address(0));
        require(to != address(this));
        uint256 b0 = token.balanceOf(to);
        try token.transfer(to, amount) returns (bool ok) {
            if (ok) {
                assert(token.balanceOf(to) == b0 + amount);
            }
        } catch {}
    }

    function check_transfer_reverts_on_insufficient_balance(
        address to,
        uint256 amount
    ) public {
        uint256 mine = token.balanceOf(address(this));
        require(amount > mine);
        try token.transfer(to, amount) returns (bool ok) {
            assert(!ok);
        } catch {}
    }
}
