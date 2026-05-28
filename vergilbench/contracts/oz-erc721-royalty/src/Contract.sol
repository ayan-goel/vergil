// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import {ERC721Royalty} from "@openzeppelin/contracts/token/ERC721/extensions/ERC721Royalty.sol";

contract Contract is ERC721Royalty {
    constructor(address receiver) ERC721("Roy721", "RY721") {
        _setDefaultRoyalty(receiver, 500); // 5%
    }
    function mint(address to, uint256 tokenId) external { _mint(to, tokenId); }
}
