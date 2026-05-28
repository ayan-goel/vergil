// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    address internal receiver = address(0xFEE);
    constructor() { c = new Contract(receiver); }

    function check_default_royalty_is_ten_percent(uint256 tokenId, uint256 price) external view {
        require(price <= 1e30);
        (address r, uint256 amount) = c.royaltyInfo(tokenId, price);
        assert(r == receiver);
        assert(amount == price * 1000 / 10000);
    }
}
