// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";

/// The ubiquitous auto-incrementing-tokenId mint pattern.
contract Contract is ERC721 {
    uint256 private _next;
    constructor() ERC721("Auto721", "AT721") {}
    function mint(address to) external returns (uint256) {
        uint256 id = _next++;
        _mint(to, id);
        return id;
    }
}
