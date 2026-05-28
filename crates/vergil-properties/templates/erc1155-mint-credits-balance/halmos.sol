// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC1155Like {
    function balanceOf(address account, uint256 id) external view returns (uint256);
    function mint(address to, uint256 id, uint256 amount) external;
}

contract Check_erc1155_mint_credits_balance {
    IERC1155Like internal token;

    function check_mint_credits_balance(address to, uint256 id, uint256 amount) external {
        require(to != address(0));
        uint256 prev = token.balanceOf(to, id);
        token.mint(to, id, amount);
        assert(token.balanceOf(to, id) == prev + amount);
    }
}
