// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import {IERC721} from "@openzeppelin/contracts/token/ERC721/IERC721.sol";
import {ERC721Wrapper} from "@openzeppelin/contracts/token/ERC721/extensions/ERC721Wrapper.sol";

contract Underlying is ERC721 {
    constructor() ERC721("Under721", "UN721") {}
    function mint(address to, uint256 id) external { _mint(to, id); }
}

contract Contract is ERC721Wrapper {
    constructor(IERC721 u) ERC721("Wrap721", "WR721") ERC721Wrapper(u) {}
}
