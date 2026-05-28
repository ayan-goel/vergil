// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import {ERC721Enumerable} from "@openzeppelin/contracts/token/ERC721/extensions/ERC721Enumerable.sol";

contract Contract is ERC721, ERC721Enumerable {
    constructor() ERC721("Enum721", "EN721") {}
    function mint(address to, uint256 tokenId) external { _mint(to, tokenId); }

    function _update(address to, uint256 tokenId, address auth)
        internal override(ERC721, ERC721Enumerable) returns (address)
    { return super._update(to, tokenId, auth); }

    function _increaseBalance(address account, uint128 value)
        internal override(ERC721, ERC721Enumerable)
    { super._increaseBalance(account, value); }

    function supportsInterface(bytes4 interfaceId)
        public view override(ERC721, ERC721Enumerable) returns (bool)
    { return super.supportsInterface(interfaceId); }
}
