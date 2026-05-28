// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    address internal receiver = address(0xFEE);
    constructor() { nft = new Contract(receiver); }

    /// Royalty is 2.5% to the configured receiver.
    function check_royalty(uint256 tokenId, uint256 price) external view {
        require(price <= 1e30);
        (address r, uint256 amount) = nft.royaltyInfo(tokenId, price);
        assert(r == receiver);
        assert(amount == price * 250 / 10000);
    }

    /// supportsInterface reports both ERC721Enumerable and ERC2981.
    function check_supports_both_interfaces() external view {
        assert(nft.supportsInterface(0x780e9d63)); // ERC721Enumerable
        assert(nft.supportsInterface(0x2a55205a)); // ERC2981
    }
}
