// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import {ERC721Consecutive} from "@openzeppelin/contracts/token/ERC721/extensions/ERC721Consecutive.sol";

/// ERC-2309 consecutive batch minting at construction.
contract Contract is ERC721Consecutive {
    constructor(address to) ERC721("Cons721", "CN721") {
        _mintConsecutive(to, 5);
    }
}
