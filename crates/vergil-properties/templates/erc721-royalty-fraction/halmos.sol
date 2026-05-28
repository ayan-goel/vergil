// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC2981Like {
    function royaltyInfo(uint256 tokenId, uint256 salePrice)
        external view returns (address, uint256);
}

contract Check_erc721_royalty_fraction {
    IERC2981Like internal token;
    address internal receiver;
    uint256 internal feeBps;

    function check_royalty_fraction(uint256 tokenId, uint256 price) external view {
        require(price <= 1e30);
        (address r, uint256 amount) = token.royaltyInfo(tokenId, price);
        assert(r == receiver);
        assert(amount == price * feeBps / 10000);
    }
}
