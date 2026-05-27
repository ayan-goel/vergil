// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITokenLike {
    function allowance(address, address) external view returns (uint256);
    function transfer(address, uint256) external returns (bool);
    function balanceOf(address) external view returns (uint256);
}

contract Check_erc20_allowance_only_via_approve {
    ITokenLike public token;

    function check_transfer_does_not_touch_allowance(
        address owner,
        address spender,
        address to,
        uint256 amount
    ) public {
        uint256 a0 = token.allowance(owner, spender);
        try token.transfer(to, amount) {} catch {}
        assert(token.allowance(owner, spender) == a0);
    }
}
