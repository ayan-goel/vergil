// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC1155ReadLike {
    function balanceOf(address account, uint256 id) external view returns (uint256);
    function totalSupply(uint256 id) external view returns (uint256);
}

contract Check_erc1155_unminted_id_zero_balance {
    IERC1155ReadLike internal token;

    function check_unminted_id_zero(address account, uint256 id) external view {
        require(token.totalSupply(id) == 0);
        assert(token.balanceOf(account, id) == 0);
    }
}
