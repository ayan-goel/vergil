// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC1155SupplyLike {
    function totalSupply(uint256 id) external view returns (uint256);
    function mint(address to, uint256 id, uint256 amount) external;
}

contract Check_erc1155_supply_tracks_mint {
    IERC1155SupplyLike internal token;

    function check_supply_tracks_mint(address to, uint256 id, uint256 amount) external {
        require(to != address(0));
        uint256 prev = token.totalSupply(id);
        token.mint(to, id, amount);
        assert(token.totalSupply(id) == prev + amount);
    }
}
