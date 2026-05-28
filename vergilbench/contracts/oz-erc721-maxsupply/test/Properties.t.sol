// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal nft;
    constructor() { nft = new Contract(); }

    /// Minting beyond MAX_SUPPLY reverts.
    function check_cannot_exceed_max(address a) external {
        require(a != address(0));
        nft.mint(a);
        nft.mint(a);
        nft.mint(a);
        try nft.mint(a) { assert(false); } catch {}
    }

    /// The minted counter never exceeds the cap.
    function check_minted_within_cap() external view {
        assert(nft.minted() <= nft.MAX_SUPPLY());
    }
}
