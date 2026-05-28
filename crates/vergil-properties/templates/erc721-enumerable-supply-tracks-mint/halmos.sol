// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC721EnumLike {
    function totalSupply() external view returns (uint256);
    function mint(address to, uint256 tokenId) external;
}

contract Check_erc721_enumerable_supply_tracks_mint {
    IERC721EnumLike internal token;

    function check_mint_increments_supply(address to, uint256 tokenId) external {
        require(to != address(0));
        uint256 prev = token.totalSupply();
        token.mint(to, tokenId);
        assert(token.totalSupply() == prev + 1);
    }
}
