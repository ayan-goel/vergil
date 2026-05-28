// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";

/// Capped-supply NFT mint (the standard "only N will ever exist" drop pattern).
contract Contract is ERC721 {
    uint256 public constant MAX_SUPPLY = 3;
    uint256 public minted;
    constructor() ERC721("Capped721", "C721") {}
    function mint(address to) external returns (uint256) {
        require(minted < MAX_SUPPLY, "sold out");
        uint256 id = minted++;
        _mint(to, id);
        return id;
    }
}
