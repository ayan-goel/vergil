// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    address internal holder = address(0xABCD);
    constructor() { nft = new Contract(holder); }

    /// Each consecutively-minted token in [0,5) is owned by the batch recipient.
    function check_consecutive_owner(uint256 tokenId) external view {
        require(tokenId < 5);
        assert(nft.ownerOf(tokenId) == holder);
    }
}
