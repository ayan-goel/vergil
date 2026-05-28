// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    address internal receiver = address(0xFEE);
    constructor() { nft = new Contract(receiver); }

    /// royaltyInfo returns 5% of the sale price to the configured receiver.
    function check_royalty_is_five_percent(uint256 tokenId, uint256 price) external view {
        require(price <= 1e30);
        (address r, uint256 amount) = nft.royaltyInfo(tokenId, price);
        assert(r == receiver);
        assert(amount == price * 500 / 10000);
    }
}
