// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";

/// Non-transferable (soulbound) token: mint/burn allowed, transfers blocked.
contract Contract is ERC721 {
    constructor() ERC721("Soul", "SBT") {}
    function mint(address to, uint256 tokenId) external { _mint(to, tokenId); }

    function _update(address to, uint256 tokenId, address auth)
        internal override returns (address)
    {
        address from = _ownerOf(tokenId);
        require(from == address(0) || to == address(0), "soulbound: non-transferable");
        return super._update(to, tokenId, auth);
    }
}
