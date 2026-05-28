// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC721UriLike {
    function ownerOf(uint256 tokenId) external view returns (address);
    function mint(address to, uint256 tokenId, string calldata uri) external;
}

contract Check_erc721_uristorage_preserves_owner {
    IERC721UriLike internal token;

    function check_mint_with_uri_sets_owner(address to, uint256 tokenId) external {
        require(to != address(0));
        token.mint(to, tokenId, "ipfs://x");
        assert(token.ownerOf(tokenId) == to);
    }
}
