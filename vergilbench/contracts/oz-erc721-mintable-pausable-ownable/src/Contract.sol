// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import {ERC721Pausable} from "@openzeppelin/contracts/token/ERC721/extensions/ERC721Pausable.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

contract Contract is ERC721Pausable, Ownable {
    uint256 private _next;
    constructor() ERC721("MintPause", "MP") Ownable(msg.sender) {}
    function safeMint(address to) external onlyOwner returns (uint256) {
        uint256 id = _next++;
        _mint(to, id);
        return id;
    }
    function pause() external onlyOwner { _pause(); }
    function unpause() external onlyOwner { _unpause(); }
}
