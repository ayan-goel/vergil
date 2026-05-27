// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITokenLike {
    function totalSupply() external view returns (uint256);
    function transfer(address, uint256) external returns (bool);
    function transferFrom(address, address, uint256) external returns (bool);
    function approve(address, uint256) external returns (bool);
}

contract Check_erc20_totalsupply_invariant {
    ITokenLike public token;

    function check_transfer_preserves_total_supply(address to, uint256 amount) public {
        uint256 t0 = token.totalSupply();
        try token.transfer(to, amount) {} catch {}
        assert(token.totalSupply() == t0);
    }

    function check_transferFrom_preserves_total_supply(
        address from,
        address to,
        uint256 amount
    ) public {
        uint256 t0 = token.totalSupply();
        try token.transferFrom(from, to, amount) {} catch {}
        assert(token.totalSupply() == t0);
    }

    function check_approve_preserves_total_supply(address spender, uint256 value) public {
        uint256 t0 = token.totalSupply();
        try token.approve(spender, value) {} catch {}
        assert(token.totalSupply() == t0);
    }
}
