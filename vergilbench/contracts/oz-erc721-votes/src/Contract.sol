// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import {ERC721Votes} from "@openzeppelin/contracts/token/ERC721/extensions/ERC721Votes.sol";
import {EIP712} from "@openzeppelin/contracts/utils/cryptography/EIP712.sol";

contract Contract is ERC721, ERC721Votes {
    constructor() ERC721("Vote721", "VT721") EIP712("Vote721", "1") {}
    function mint(address to, uint256 tokenId) external { _mint(to, tokenId); }

    function _update(address to, uint256 tokenId, address auth)
        internal override(ERC721, ERC721Votes) returns (address)
    { return super._update(to, tokenId, auth); }

    function _increaseBalance(address account, uint128 value)
        internal override(ERC721, ERC721Votes)
    { super._increaseBalance(account, value); }
}
