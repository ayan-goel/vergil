// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import {ERC721Burnable} from "@openzeppelin/contracts/token/ERC721/extensions/ERC721Burnable.sol";

contract Contract is ERC721Burnable {
    constructor() ERC721("Burn721", "BN721") {}
    function mint(address to, uint256 tokenId) external { _mint(to, tokenId); }
}
