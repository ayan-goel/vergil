// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IMintBurnLike {
    function balanceOf(address) external view returns (uint256);
    function totalSupply() external view returns (uint256);
    function mint(address, uint256) external;
    function burn(uint256) external;
}

contract Check_erc20_mint_burn_accounting {
    IMintBurnLike public token;

    function check_mint_credits_recipient_and_total(address to, uint256 amount) public {
        require(to != address(0));
        uint256 b0 = token.balanceOf(to);
        uint256 t0 = token.totalSupply();
        try token.mint(to, amount) {
            assert(token.balanceOf(to) == b0 + amount);
            assert(token.totalSupply() == t0 + amount);
        } catch {}
    }

    function check_burn_debits_self_and_total(uint256 amount) public {
        uint256 b0 = token.balanceOf(address(this));
        uint256 t0 = token.totalSupply();
        require(amount <= b0);
        try token.burn(amount) {
            assert(token.balanceOf(address(this)) == b0 - amount);
            assert(token.totalSupply() == t0 - amount);
        } catch {}
    }
}
