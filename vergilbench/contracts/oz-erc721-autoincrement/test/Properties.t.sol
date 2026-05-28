// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// Consecutive mints receive consecutive, increasing token ids.
    function check_ids_autoincrement(address a, address b) external {
        require(a != address(0) && b != address(0));
        uint256 id1 = nft.mint(a);
        uint256 id2 = nft.mint(b);
        assert(id2 == id1 + 1);
        assert(nft.ownerOf(id1) == a);
        assert(nft.ownerOf(id2) == b);
    }
}
