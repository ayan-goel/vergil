// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC1155BurnLike {
    function balanceOf(address account, uint256 id) external view returns (uint256);
    function burn(address from, uint256 id, uint256 amount) external;
}

contract Check_erc1155_burn_debits_balance {
    IERC1155BurnLike internal token;

    function check_burn_debits_balance(address from, uint256 id, uint256 amount) external {
        uint256 prev = token.balanceOf(from, id);
        require(amount <= prev);
        token.burn(from, id, amount);
        assert(token.balanceOf(from, id) == prev - amount);
    }
}
