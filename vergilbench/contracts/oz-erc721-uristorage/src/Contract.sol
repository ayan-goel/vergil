// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import {ERC721URIStorage} from "@openzeppelin/contracts/token/ERC721/extensions/ERC721URIStorage.sol";

contract Contract is ERC721URIStorage {
    constructor() ERC721("Uri721", "UR721") {}
    function mint(address to, uint256 tokenId, string calldata uri) external {
        _mint(to, tokenId);
        _setTokenURI(tokenId, uri);
    }
}
